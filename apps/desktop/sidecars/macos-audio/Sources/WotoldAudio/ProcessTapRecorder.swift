import AVFoundation
import CoreAudio
import Foundation

// macOS 14.4+ Core Audio Process Tap для захвата ВСЕГО системного аудио,
// включая приложения которые маршрутизируют звук в обход системного микса
// (FaceTime, Zoom, Teams). Заменяет ScreenCaptureKit (SystemAudioRecorder),
// который терял FaceTime/Zoom audio из-за privacy-by-design.
//
// Pipeline:
//   1. CATapDescription(monoGlobalTapButExcludeProcesses: []) — глобальный
//      mono mixdown всего system audio.
//   2. AudioHardwareCreateProcessTap → tapID.
//   3. AudioHardwareCreateAggregateDevice с этим tap → aggregateID.
//   4. AudioDeviceCreateIOProcIDWithBlock — receive Float32 buffers в
//      callback.
//   5. AVAudioConverter → 16kHz mono Int16.
//   6. WAVWriter → system.wav (тот же writer + 5s flush header что был для
//      SCStream-варианта).
//
// Permission: macOS 14.4+ объединил эту функциональность под существующий
// TCC service kAudioServiceScreenCapture («Screen & System Audio Recording»
// в System Settings → Privacy). Поэтому Permissions.swift не меняется —
// проверка CGPreflightScreenCaptureAccess работает для обоих.

@available(macOS 14.4, *)
final class ProcessTapRecorder: NSObject {
    struct StopResult {
        let bytesWritten: UInt64
    }

    private var tapID: AudioObjectID = kAudioObjectUnknown
    private var aggregateID: AudioObjectID = kAudioObjectUnknown
    private var ioProcID: AudioDeviceIOProcID?
    private var wavWriter: WAVWriter?
    private var converter: AVAudioConverter?
    private var outputFormat: AVAudioFormat?
    private var inputFormat: AVAudioFormat?
    private var flushTimer: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "app.wotold.macos-audio.tap")
    private let flushInterval: TimeInterval = 5.0
    private(set) var bytesWritten: UInt64 = 0

    // [B14] Running RMS для live level meter — тот же контракт что был у
    // SystemAudioRecorder.currentRms, чтобы App.swift не различал.
    private var latestRms: Float = 0
    var currentRms: Float { latestRms }

    func start(systemURL: URL) async throws {
        guard let outFormat = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: 16_000,
            channels: 1,
            interleaved: true
        ) else {
            throw Self.error("failed to create output format (16kHz mono i16)")
        }

        // 1. Описать глобальный mono tap. Пустой массив excludes = ловить
        // ВСЁ system audio. muteBehavior=.unmuted чтобы юзер продолжал
        // слышать собеседника.
        let description = CATapDescription(monoGlobalTapButExcludeProcesses: [])
        description.name = "Wotold System Audio"
        description.uuid = UUID()
        description.isPrivate = true
        description.muteBehavior = .unmuted

        // 2. Создать tap. Первый вызов триггерит macOS-диалог Screen
        // Recording если он ещё не granted.
        var newTapID: AudioObjectID = kAudioObjectUnknown
        var status = AudioHardwareCreateProcessTap(description, &newTapID)
        guard status == noErr, newTapID != kAudioObjectUnknown else {
            throw Self.error(
                "AudioHardwareCreateProcessTap failed (\(status)). "
                    + "Открой System Settings → Privacy & Security → Screen & System Audio "
                    + "Recording и включи Wotold, потом перезапусти приложение."
            )
        }

        // 3. Получить UID tap'а для aggregate device.
        let tapUID: CFString
        do {
            tapUID = try Self.getStringProperty(
                objectID: newTapID,
                selector: kAudioTapPropertyUID
            )
        } catch {
            AudioHardwareDestroyProcessTap(newTapID)
            throw error
        }

        // 4. Запросить формат tap'а — обычно Float32 mono @ 48kHz.
        let inFormat: AVAudioFormat
        do {
            inFormat = try Self.getStreamFormat(objectID: newTapID)
        } catch {
            AudioHardwareDestroyProcessTap(newTapID)
            throw error
        }

        // 5. Создать aggregate device, включающий этот tap. Aggregate нужен
        // потому что IOProc вешается на устройство, а tap — это не device.
        let aggregateDesc: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Wotold Tap Aggregate",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceIsPrivateKey: 1,
            kAudioAggregateDeviceIsStackedKey: 0,
            kAudioAggregateDeviceTapListKey: [[
                kAudioSubTapUIDKey: tapUID,
                kAudioSubTapDriftCompensationKey: 1,
            ]],
        ]
        var newAggregateID: AudioObjectID = kAudioObjectUnknown
        status = AudioHardwareCreateAggregateDevice(
            aggregateDesc as CFDictionary,
            &newAggregateID
        )
        guard status == noErr, newAggregateID != kAudioObjectUnknown else {
            AudioHardwareDestroyProcessTap(newTapID)
            throw Self.error("AudioHardwareCreateAggregateDevice failed (\(status))")
        }

        // 6. Открыть WAV writer.
        let writer: WAVWriter
        do {
            writer = try WAVWriter(url: systemURL, sampleRate: 16_000, channels: 1)
        } catch {
            AudioHardwareDestroyAggregateDevice(newAggregateID)
            AudioHardwareDestroyProcessTap(newTapID)
            throw error
        }

        // 7. Подписаться на аудио-буферы через IOProc.
        var procID: AudioDeviceIOProcID?
        status = AudioDeviceCreateIOProcIDWithBlock(&procID, newAggregateID, queue) {
            [weak self] _, inInputData, _, _, _ in
            self?.handleAudio(inputData: inInputData)
        }
        guard status == noErr, let validProcID = procID else {
            try? writer.close()
            AudioHardwareDestroyAggregateDevice(newAggregateID)
            AudioHardwareDestroyProcessTap(newTapID)
            throw Self.error("AudioDeviceCreateIOProcIDWithBlock failed (\(status))")
        }

        // 8. Стартуем IO.
        status = AudioDeviceStart(newAggregateID, validProcID)
        guard status == noErr else {
            AudioDeviceDestroyIOProcID(newAggregateID, validProcID)
            try? writer.close()
            AudioHardwareDestroyAggregateDevice(newAggregateID)
            AudioHardwareDestroyProcessTap(newTapID)
            throw Self.error("AudioDeviceStart failed (\(status))")
        }

        // Стор всех ресурсов в инстансе.
        self.tapID = newTapID
        self.aggregateID = newAggregateID
        self.ioProcID = validProcID
        self.wavWriter = writer
        self.outputFormat = outFormat
        self.inputFormat = inFormat
        self.bytesWritten = 0

        // M1.5: периодический flush header на диск для crash-safety.
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + flushInterval, repeating: flushInterval)
        timer.setEventHandler { [weak self] in
            guard let w = self?.wavWriter else { return }
            try? w.flushHeader()
        }
        timer.resume()
        flushTimer = timer
    }

    /// [M13] Атомарно завершает текущий chunk WAV и открывает новый. IOProc
    /// остаётся активным, handleAudio продолжает писать в self.wavWriter
    /// который мы swap'аем. Sync на queue гарантирует что между close-old и
    /// open-new в IOProc callback не зайдёт другой buffer (callback тоже
    /// queue-bound через AudioDeviceCreateIOProcIDWithBlock).
    /// Возвращает bytesWritten ПРЕДЫДУЩЕГО chunk'а.
    func rotate(to url: URL) throws -> UInt64 {
        return try queue.sync {
            guard aggregateID != kAudioObjectUnknown, ioProcID != nil else {
                throw Self.error("rotate called before start")
            }
            try wavWriter?.close()
            let oldBytes = bytesWritten

            let newWriter = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
            wavWriter = newWriter
            bytesWritten = 0
            // converter reuse — input format не меняется в pre-stop period.
            return oldBytes
        }
    }

    func stop() async throws -> StopResult {
        flushTimer?.cancel()
        flushTimer = nil

        if aggregateID != kAudioObjectUnknown, let procID = ioProcID {
            _ = AudioDeviceStop(aggregateID, procID)
            _ = AudioDeviceDestroyIOProcID(aggregateID, procID)
        }
        if aggregateID != kAudioObjectUnknown {
            _ = AudioHardwareDestroyAggregateDevice(aggregateID)
        }
        if tapID != kAudioObjectUnknown {
            _ = AudioHardwareDestroyProcessTap(tapID)
        }
        try wavWriter?.close()

        let bytes = bytesWritten
        aggregateID = kAudioObjectUnknown
        tapID = kAudioObjectUnknown
        ioProcID = nil
        wavWriter = nil
        converter = nil
        outputFormat = nil
        inputFormat = nil
        bytesWritten = 0
        latestRms = 0

        return StopResult(bytesWritten: bytes)
    }

    // MARK: - IOProc

    private func handleAudio(inputData: UnsafePointer<AudioBufferList>) {
        guard let inFormat = inputFormat,
              let outFormat = outputFormat,
              let writer = wavWriter
        else { return }

        // Bytes per frame из ASBD. Если 0 — формат битый, дропаем.
        let bytesPerFrame = inFormat.streamDescription.pointee.mBytesPerFrame
        if bytesPerFrame == 0 { return }

        let firstBufferSize = inputData.pointee.mBuffers.mDataByteSize
        let frameCount = AVAudioFrameCount(firstBufferSize / bytesPerFrame)
        if frameCount == 0 { return }

        guard let pcm = AVAudioPCMBuffer(
            pcmFormat: inFormat,
            bufferListNoCopy: inputData,
            deallocator: nil
        ) else { return }
        pcm.frameLength = frameCount

        if converter == nil {
            converter = AVAudioConverter(from: inFormat, to: outFormat)
        }
        guard let conv = converter else { return }

        let ratio = outFormat.sampleRate / inFormat.sampleRate
        let capacity = AVAudioFrameCount(Double(frameCount) * ratio) + 1024
        guard let outBuffer = AVAudioPCMBuffer(pcmFormat: outFormat, frameCapacity: capacity)
        else { return }

        var consumed = false
        var error: NSError?
        let status = conv.convert(to: outBuffer, error: &error) { _, outStatus in
            if consumed {
                outStatus.pointee = .noDataNow
                return nil
            }
            consumed = true
            outStatus.pointee = .haveData
            return pcm
        }

        if status == .error || error != nil {
            FileHandle.standardError.write(
                Data("system convert error: \(error?.localizedDescription ?? "?")\n".utf8)
            )
            return
        }
        if outBuffer.frameLength == 0 { return }

        do {
            let written = try writer.write(buffer: outBuffer)
            bytesWritten &+= UInt64(written)
        } catch {
            FileHandle.standardError.write(
                Data("system wav write error: \(error.localizedDescription)\n".utf8)
            )
        }

        latestRms = computeInt16Rms(outBuffer)
    }

    // MARK: - Core Audio helpers

    private static func error(_ message: String) -> NSError {
        NSError(
            domain: "WotoldAudio.Tap",
            code: 1,
            userInfo: [NSLocalizedDescriptionKey: message]
        )
    }

    private static func getStringProperty(
        objectID: AudioObjectID,
        selector: AudioObjectPropertySelector
    ) throws -> CFString {
        var address = AudioObjectPropertyAddress(
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var size: UInt32 = UInt32(MemoryLayout<CFString>.size)
        var value: Unmanaged<CFString>?
        let status = withUnsafeMutablePointer(to: &value) { ptr in
            AudioObjectGetPropertyData(objectID, &address, 0, nil, &size, ptr)
        }
        guard status == noErr, let unmanaged = value else {
            throw error("AudioObjectGetPropertyData(CFString) failed (\(status))")
        }
        return unmanaged.takeRetainedValue()
    }

    private static func getStreamFormat(objectID: AudioObjectID) throws -> AVAudioFormat {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioTapPropertyFormat,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var asbd = AudioStreamBasicDescription()
        var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        let status = AudioObjectGetPropertyData(objectID, &address, 0, nil, &size, &asbd)
        guard status == noErr else {
            throw error("AudioObjectGetPropertyData(StreamFormat) failed (\(status))")
        }
        guard let format = AVAudioFormat(streamDescription: &asbd) else {
            throw error("AVAudioFormat init from ASBD failed")
        }
        return format
    }
}
