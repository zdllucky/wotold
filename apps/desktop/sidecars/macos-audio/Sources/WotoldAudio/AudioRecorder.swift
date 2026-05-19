import AVFoundation
import Foundation

// AVAudioEngine читает с inputNode в его «нативном» формате (обычно 44.1/48 kHz float32).
// Конвертируем в 16-bit PCM 16 kHz mono на лету через AVAudioConverter — это
// формат который Soniox/Gladia принимают без потерь и который ожидают M1.2/M2.4 паспорта.

final class AudioRecorder {
    private var engine: AVAudioEngine?
    private var wavWriter: WAVWriter?
    private var converter: AVAudioConverter?
    private var outputFormat: AVAudioFormat?
    private var inputFormat: AVAudioFormat?
    private var startTime: Date?
    private var bytesWritten: UInt64 = 0
    private let queue = DispatchQueue(label: "app.wotold.macos-audio.recorder")

    func start(micURL: URL) throws {
        // Если уже пишем — стопаем предыдущий чтобы не потерять состояние.
        if engine != nil {
            _ = try? stop()
        }

        let engine = AVAudioEngine()
        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)

        guard
            let outFormat = AVAudioFormat(
                commonFormat: .pcmFormatInt16,
                sampleRate: 16_000,
                channels: 1,
                interleaved: true
            )
        else {
            throw NSError(
                domain: "WotoldAudio",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "failed to create output format"]
            )
        }

        guard let conv = AVAudioConverter(from: inputFormat, to: outFormat) else {
            throw NSError(
                domain: "WotoldAudio",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "failed to create AVAudioConverter"]
            )
        }

        let writer = try WAVWriter(url: micURL, sampleRate: 16_000, channels: 1)

        input.installTap(onBus: 0, bufferSize: 4096, format: inputFormat) {
            [weak self] inBuffer, _ in
            guard let self else { return }
            self.queue.async { [weak self] in
                self?.processBuffer(inBuffer, converter: conv, outFormat: outFormat, writer: writer)
            }
        }

        try engine.start()

        self.engine = engine
        self.wavWriter = writer
        self.converter = conv
        self.outputFormat = outFormat
        self.inputFormat = inputFormat
        self.startTime = Date()
        self.bytesWritten = 0
    }

    private func processBuffer(
        _ inBuffer: AVAudioPCMBuffer,
        converter: AVAudioConverter,
        outFormat: AVAudioFormat,
        writer: WAVWriter
    ) {
        // Грубая оценка capacity: ratio sample rates × входные кадры + запас.
        let ratio = outFormat.sampleRate / inBuffer.format.sampleRate
        let capacity = AVAudioFrameCount(Double(inBuffer.frameLength) * ratio) + 1024

        guard let outBuffer = AVAudioPCMBuffer(pcmFormat: outFormat, frameCapacity: capacity) else {
            return
        }

        var consumed = false
        var error: NSError?
        let status = converter.convert(to: outBuffer, error: &error) { _, outStatus in
            if consumed {
                outStatus.pointee = .noDataNow
                return nil
            }
            consumed = true
            outStatus.pointee = .haveData
            return inBuffer
        }

        if status == .error || error != nil {
            FileHandle.standardError.write(
                Data("audio convert error: \(error?.localizedDescription ?? "?")\n".utf8)
            )
            return
        }

        if outBuffer.frameLength == 0 { return }

        do {
            let bytes = try writer.write(buffer: outBuffer)
            bytesWritten += UInt64(bytes)
        } catch {
            FileHandle.standardError.write(
                Data("wav write error: \(error.localizedDescription)\n".utf8)
            )
        }
    }

    func stop() throws -> (durationSec: Double, micBytes: UInt64) {
        engine?.inputNode.removeTap(onBus: 0)
        engine?.stop()
        try wavWriter?.close()

        let duration = startTime.map { Date().timeIntervalSince($0) } ?? 0
        let bytes = bytesWritten

        engine = nil
        wavWriter = nil
        converter = nil
        outputFormat = nil
        inputFormat = nil
        startTime = nil
        bytesWritten = 0

        return (durationSec: duration, micBytes: bytes)
    }
}
