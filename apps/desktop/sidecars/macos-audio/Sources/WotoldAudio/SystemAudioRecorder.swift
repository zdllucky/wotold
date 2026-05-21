import AVFoundation
import Foundation
import ScreenCaptureKit

// Захват системного выхода через ScreenCaptureKit (macOS 13+). Видео нам
// не нужно, но SCStream требует хотя бы один stream output — добавляем
// .audio и просим минимальный 2x2 video при 1fps чтобы не тратить CPU.
//
// Первый вызов SCShareableContent.current триггерит macOS-диалог запроса
// разрешения Screen Recording. После одобрения в System Settings → Privacy
// и перезапуска приложения работает без вопросов.

final class SystemAudioRecorder: NSObject, SCStreamOutput, SCStreamDelegate {
    struct StopResult {
        let bytesWritten: UInt64
    }

    private var stream: SCStream?
    private var wavWriter: WAVWriter?
    private var converter: AVAudioConverter?
    private var outputFormat: AVAudioFormat?
    private var flushTimer: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "app.wotold.macos-audio.system")
    private let flushInterval: TimeInterval = 5.0
    private(set) var bytesWritten: UInt64 = 0

    // [B14] Running RMS для live level meter.
    private var latestRms: Float = 0
    var currentRms: Float { latestRms }

    func start(systemURL: URL) async throws {
        guard
            let outFormat = AVAudioFormat(
                commonFormat: .pcmFormatInt16,
                sampleRate: 16_000,
                channels: 1,
                interleaved: true
            )
        else {
            throw NSError(
                domain: "WotoldAudio.System",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "failed to create output format"]
            )
        }

        let content: SCShareableContent
        do {
            content = try await SCShareableContent.excludingDesktopWindows(
                false,
                onScreenWindowsOnly: false
            )
        } catch {
            throw NSError(
                domain: "WotoldAudio.System",
                code: 10,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Screen Recording permission denied. Открой System Settings → Privacy & Security → Screen Recording и включи Wotold, потом перезапусти приложение."
                ]
            )
        }

        guard let display = content.displays.first else {
            throw NSError(
                domain: "WotoldAudio.System",
                code: 11,
                userInfo: [NSLocalizedDescriptionKey: "no display for system audio capture"]
            )
        }

        let filter = SCContentFilter(
            display: display,
            excludingApplications: [],
            exceptingWindows: []
        )

        let config = SCStreamConfiguration()
        config.capturesAudio = true
        config.sampleRate = 48_000
        config.channelCount = 2
        config.excludesCurrentProcessAudio = true
        config.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        config.width = 2
        config.height = 2

        let stream = SCStream(filter: filter, configuration: config, delegate: self)
        try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: queue)

        wavWriter = try WAVWriter(url: systemURL, sampleRate: 16_000, channels: 1)
        outputFormat = outFormat
        bytesWritten = 0

        try await stream.startCapture()
        self.stream = stream

        // M1.5: периодический flush header на диск для crash-safety.
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + flushInterval, repeating: flushInterval)
        timer.setEventHandler { [weak self] in
            guard let writer = self?.wavWriter else { return }
            try? writer.flushHeader()
        }
        timer.resume()
        flushTimer = timer
    }

    func stop() async throws -> StopResult {
        flushTimer?.cancel()
        flushTimer = nil

        if let stream = stream {
            try await stream.stopCapture()
        }
        try wavWriter?.close()

        let bytes = bytesWritten
        stream = nil
        wavWriter = nil
        converter = nil
        outputFormat = nil
        bytesWritten = 0
        latestRms = 0

        return StopResult(bytesWritten: bytes)
    }

    // MARK: - SCStreamOutput

    func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of type: SCStreamOutputType
    ) {
        guard type == .audio, sampleBuffer.isValid, sampleBuffer.numSamples > 0 else { return }
        guard let pcm = sampleBuffer.toPCMBuffer() else { return }
        guard let outFormat = outputFormat, let writer = wavWriter else { return }

        if converter == nil {
            converter = AVAudioConverter(from: pcm.format, to: outFormat)
        }
        guard let conv = converter else { return }

        let ratio = outFormat.sampleRate / pcm.format.sampleRate
        let capacity = AVAudioFrameCount(Double(pcm.frameLength) * ratio) + 1024
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

        // [B14] RMS post-write для live level meter.
        latestRms = computeInt16Rms(outBuffer)
    }

    // MARK: - SCStreamDelegate

    func stream(_ stream: SCStream, didStopWithError error: any Error) {
        FileHandle.standardError.write(
            Data("system stream stopped with error: \(error)\n".utf8)
        )
    }
}

private extension CMSampleBuffer {
    func toPCMBuffer() -> AVAudioPCMBuffer? {
        guard let formatDesc = self.formatDescription,
              let asbdPtr = CMAudioFormatDescriptionGetStreamBasicDescription(formatDesc)
        else { return nil }
        var asbd = asbdPtr.pointee
        guard let format = AVAudioFormat(streamDescription: &asbd) else { return nil }

        let numFrames = AVAudioFrameCount(self.numSamples)
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: numFrames) else {
            return nil
        }
        buffer.frameLength = numFrames

        let status = CMSampleBufferCopyPCMDataIntoAudioBufferList(
            self,
            at: 0,
            frameCount: Int32(numFrames),
            into: buffer.mutableAudioBufferList
        )
        guard status == noErr else { return nil }
        return buffer
    }
}
