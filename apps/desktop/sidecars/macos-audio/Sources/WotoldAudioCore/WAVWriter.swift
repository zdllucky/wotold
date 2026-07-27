import AVFoundation
import Foundation

// Минимальный RIFF/WAV writer для PCM16. Пишет placeholder-заголовок
// при init, периодически (через AudioRecorder) обновляет размеры в
// flushHeader() — M1.5: краш не уничтожает уже записанное. Окончательное
// закрытие — в close(). Не thread-safe — вызывается из последовательной
// очереди AudioRecorder/SystemAudioRecorder.

/// [TD-21] Ошибки writer'а, которые вызывающему полезно различать.
public enum WAVWriterError: Error, Equatable {
    /// Достигнут потолок RIFF — кадр не записан, файл остаётся валидным.
    case sizeLimitReached
}

public final class WAVWriter {
    /// [TD-21] Потолок полезной нагрузки RIFF. Размеры в заголовке — 32-битные,
    /// и `RIFF size` = `dataBytes + 36`, поэтому упереться можно чуть раньше
    /// 4 GiB. При 16 кГц mono int16 (32 000 байт/с) это ~37 часов записи.
    ///
    /// Раньше счётчик был `UInt32` с `&+=`: после переполнения заголовок молча
    /// становился мусором, и файл переставал открываться целиком. Теперь
    /// счётчик 64-битный, а запись сверх лимита отклоняется явной ошибкой —
    /// формат всё равно больше не вмещает (правило 3: деградация видима).
    public static let maxDataBytes: UInt64 = UInt64(UInt32.max) - 36

    private let handle: FileHandle
    private let sampleRate: UInt32
    private let channels: UInt16
    private var dataBytes: UInt64 = 0
    /// Однократный признак «лимит достигнут» — чтобы не спамить в stderr на
    /// каждом кадре и чтобы `close()` знал, что файл усечён.
    private(set) public var didHitLimit = false

    public init(url: URL, sampleRate: UInt32, channels: UInt16) throws {
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
    public func write(buffer: AVAudioPCMBuffer) throws -> Int {
        guard let channelData = buffer.int16ChannelData else { return 0 }
        let channelCount = Int(buffer.format.channelCount)
        let frameLength = Int(buffer.frameLength)
        let totalSamples = frameLength * channelCount
        let byteCount = totalSamples * MemoryLayout<Int16>.size

        let pointer = channelData[0]
        let data = Data(bytes: UnsafeRawPointer(pointer), count: byteCount)
        do {
            try appendBytes(data)
        } catch WAVWriterError.sizeLimitReached {
            // Не фатально и не повод спамить: про лимит уже сказано один раз
            // в `appendBytes`, состояние читается через `didHitLimit`.
            // Возвращаем 0 — «кадр не записан», счётчики звонка не врут.
            return 0
        }
        return byteCount
    }

    /// [TD-21] Тестовый шов: подвести счётчик к границе, не записывая на диск
    /// реальные 4 GiB. `internal`, поэтому виден только тестам через
    /// `@testable import` — исполняемый таргет это другой модуль и вызвать
    /// такое не сможет.
    func primeDataBytesForTesting(_ value: UInt64) {
        dataBytes = value
    }

    /// [TD-21] Общий путь записи с проверкой потолка RIFF. Выделен, чтобы
    /// лимит нельзя было обойти, добавив вторую точку записи.
    func appendBytes(_ data: Data) throws {
        guard !data.isEmpty else { return }
        let next = dataBytes + UInt64(data.count)
        if next > Self.maxDataBytes {
            if !didHitLimit {
                didHitLimit = true
                let msg = "wav writer: достигнут потолок RIFF (\(Self.maxDataBytes) байт), "
                    + "дальнейшие кадры отбрасываются — файл останется валидным\n"
                FileHandle.standardError.write(Data(msg.utf8))
            }
            throw WAVWriterError.sizeLimitReached
        }
        try handle.write(contentsOf: data)
        dataBytes = next
    }

    /// Перезаписывает RIFF/data размеры под текущее `dataBytes` без закрытия файла.
    /// Курсор возвращается на конец, синхронизация на диск через synchronize().
    /// M1.5 паспорта: краш между периодическими flush'ами оставляет валидный
    /// WAV до последнего успешного flush'а.
    public func flushHeader() throws {
        // [TD-21] Счётчик 64-битный, в заголовок идут заведомо влезающие
        // значения: `appendBytes` не даёт превысить `maxDataBytes`, так что
        // сужение здесь всегда точное, а не обрезающее.
        let size = UInt32(truncatingIfNeeded: min(dataBytes, Self.maxDataBytes))
        let endOffset = try handle.offset()
        try handle.seek(toOffset: 4)
        try handle.write(contentsOf: u32(size + 36))
        try handle.seek(toOffset: 40)
        try handle.write(contentsOf: u32(size))
        try handle.seek(toOffset: endOffset)
        try handle.synchronize()
    }

    public func close() throws {
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
