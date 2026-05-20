import AVFoundation
import Foundation

// Минимальный RIFF/WAV writer для PCM16. Пишет placeholder-заголовок
// при init, периодически (через AudioRecorder) обновляет размеры в
// flushHeader() — M1.5: краш не уничтожает уже записанное. Окончательное
// закрытие — в close(). Не thread-safe — вызывается из последовательной
// очереди AudioRecorder/SystemAudioRecorder.

final class WAVWriter {
    private let handle: FileHandle
    private let sampleRate: UInt32
    private let channels: UInt16
    private var dataBytes: UInt32 = 0

    init(url: URL, sampleRate: UInt32, channels: UInt16) throws {
        self.sampleRate = sampleRate
        self.channels = channels

        let parent = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(
            at: parent,
            withIntermediateDirectories: true
        )
        try? FileManager.default.removeItem(at: url)
        FileManager.default.createFile(atPath: url.path, contents: nil)

        guard let h = try? FileHandle(forWritingTo: url) else {
            throw NSError(
                domain: "WotoldAudio.WAVWriter",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "open file \(url.path)"]
            )
        }
        self.handle = h
        try writePlaceholderHeader()
    }

    private func writePlaceholderHeader() throws {
        let bitsPerSample: UInt16 = 16
        let byteRate = sampleRate * UInt32(channels) * UInt32(bitsPerSample / 8)
        let blockAlign = channels * (bitsPerSample / 8)

        var data = Data()
        data.append(Data("RIFF".utf8))
        data.append(u32(0))  // placeholder: file size - 8
        data.append(Data("WAVE".utf8))

        data.append(Data("fmt ".utf8))
        data.append(u32(16))  // PCM fmt chunk size
        data.append(u16(1))   // PCM format
        data.append(u16(channels))
        data.append(u32(sampleRate))
        data.append(u32(byteRate))
        data.append(u16(blockAlign))
        data.append(u16(bitsPerSample))

        data.append(Data("data".utf8))
        data.append(u32(0))  // placeholder: data size

        try handle.write(contentsOf: data)
    }

    /// Пишет содержимое interleaved Int16 буфера, возвращает количество записанных байт.
    func write(buffer: AVAudioPCMBuffer) throws -> Int {
        guard let channelData = buffer.int16ChannelData else { return 0 }
        let channelCount = Int(buffer.format.channelCount)
        let frameLength = Int(buffer.frameLength)
        let totalSamples = frameLength * channelCount
        let byteCount = totalSamples * MemoryLayout<Int16>.size

        let pointer = channelData[0]
        let data = Data(bytes: UnsafeRawPointer(pointer), count: byteCount)
        try handle.write(contentsOf: data)

        dataBytes &+= UInt32(byteCount)
        return byteCount
    }

    /// Перезаписывает RIFF/data размеры под текущее `dataBytes` без закрытия файла.
    /// Курсор возвращается на конец, синхронизация на диск через synchronize().
    /// M1.5 паспорта: краш между периодическими flush'ами оставляет валидный
    /// WAV до последнего успешного flush'а.
    func flushHeader() throws {
        let endOffset = try handle.offset()
        try handle.seek(toOffset: 4)
        try handle.write(contentsOf: u32(dataBytes &+ 36))
        try handle.seek(toOffset: 40)
        try handle.write(contentsOf: u32(dataBytes))
        try handle.seek(toOffset: endOffset)
        try handle.synchronize()
    }

    func close() throws {
        try flushHeader()
        try handle.close()
    }

    private func u32(_ v: UInt32) -> Data {
        var le = v.littleEndian
        return Data(bytes: &le, count: 4)
    }

    private func u16(_ v: UInt16) -> Data {
        var le = v.littleEndian
        return Data(bytes: &le, count: 2)
    }
}
