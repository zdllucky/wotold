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
    private var flushTimer: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "app.wotold.macos-audio.recorder")
    private let flushInterval: TimeInterval = 5.0

    // [B14] Running RMS for live level meter. Frontend reads через
    // {"event":"level","mic":..,"system":..} stdout эмит'ы каждые 100ms.
    // Не thread-safe чтение из main thread — но atomic-fast enough.
    private var latestRms: Float = 0
    var currentRms: Float { latestRms }

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

        // [M13] processBuffer читает writer/converter/outFormat из self.*, а не
        // из captured params — это позволяет атомарно swap'нуть self.wavWriter
        // в rotate(to:) без замены закрытого tap'а.
        input.installTap(onBus: 0, bufferSize: 4096, format: inputFormat) {
            [weak self] inBuffer, _ in
            guard let self else { return }
            self.queue.async { [weak self] in
                self?.processBuffer(inBuffer)
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

        // M1.5: периодический flush WAV-заголовка на диск — если процесс
        // упадёт, файл остаётся валидным до последнего успешного flush'а.
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + flushInterval, repeating: flushInterval)
        timer.setEventHandler { [weak self] in
            guard let writer = self?.wavWriter else { return }
            try? writer.flushHeader()
        }
        timer.resume()
        flushTimer = timer
    }

    private func processBuffer(_ inBuffer: AVAudioPCMBuffer) {
        // Reads writer/converter/outFormat из self.* — rotate(to:) может
        // атомарно swap'нуть wavWriter под нами без замены tap'а.
        guard let converter = self.converter,
              let outFormat = self.outputFormat,
              let writer = self.wavWriter
        else { return }

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

        // [B14] RMS post-write — frontend читает latestRms через эмит таймер.
        latestRms = computeInt16Rms(outBuffer)
    }

    /// [M13] Атомарно завершает текущий chunk WAV и открывает новый. Tap
    /// остаётся installed, processBuffer продолжает писать в self.wavWriter
    /// который мы только что подменили. Sync executes на queue — гарантирует
    /// что между close-old и open-new в processBuffer не зайдёт другой buffer.
    /// Возвращает duration + bytes ПРЕДЫДУЩЕГО chunk'а.
    func rotate(to url: URL) throws -> (durationSec: Double, micBytes: UInt64) {
        return try queue.sync { [weak self] in
            guard let self = self, self.engine != nil else {
                throw NSError(
                    domain: "WotoldAudio",
                    code: 10,
                    userInfo: [NSLocalizedDescriptionKey: "rotate called before start"]
                )
            }
            // Close current chunk.
            try self.wavWriter?.close()
            let oldDuration = self.startTime.map { Date().timeIntervalSince($0) } ?? 0
            let oldBytes = self.bytesWritten

            // Open new chunk WAV — same format (16kHz mono i16).
            let newWriter = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
            self.wavWriter = newWriter
            self.startTime = Date()
            self.bytesWritten = 0
            return (durationSec: oldDuration, micBytes: oldBytes)
        }
    }

    func stop() throws -> (durationSec: Double, micBytes: UInt64) {
        flushTimer?.cancel()
        flushTimer = nil

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
        latestRms = 0

        return (durationSec: duration, micBytes: bytes)
    }
}

// [B14] Compute RMS из int16 PCM AVAudioPCMBuffer, нормализован 0..1.
func computeInt16Rms(_ buffer: AVAudioPCMBuffer) -> Float {
    guard buffer.format.commonFormat == .pcmFormatInt16,
          let data = buffer.int16ChannelData
    else { return 0 }
    let channel = data[0]
    let count = Int(buffer.frameLength)
    guard count > 0 else { return 0 }
    var sumSq: Double = 0
    for i in 0..<count {
        let v = Double(channel[i]) / 32768.0
        sumSq += v * v
    }
    let rms = sqrt(sumSq / Double(count))
    // Clamp 0..1 — иногда float overflow на пик ~1.05.
    return Float(min(1.0, max(0.0, rms)))
}
