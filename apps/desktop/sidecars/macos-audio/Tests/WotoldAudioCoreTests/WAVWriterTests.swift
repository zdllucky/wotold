import AVFoundation
import Foundation
import Testing

@testable import WotoldAudioCore

/// [TD-21] WAVWriter раньше вообще не был покрыт: тесты ядра проверяли только
/// протокол и роутер. При этом именно он пишет файл, ради которого существует
/// весь сайдкар.
@Suite("WAVWriter — потолок RIFF (TD-21c)")
struct WAVWriterLimitTests {

    private func tempURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("wotold-test-\(UUID().uuidString).wav")
    }

    @Test("заголовок пустого файла валиден и говорит о нуле данных")
    func emptyHeaderIsValid() throws {
        let url = tempURL()
        defer { try? FileManager.default.removeItem(at: url) }
        let w = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
        try w.close()

        let bytes = try Data(contentsOf: url)
        #expect(bytes.count == 44, "канонический PCM-заголовок — 44 байта")
        #expect(bytes.prefix(4) == Data("RIFF".utf8))
        #expect(bytes[8..<12] == Data("WAVE".utf8))
        // RIFF size = 36 при нулевых данных, data size = 0.
        #expect(u32le(bytes, at: 4) == 36)
        #expect(u32le(bytes, at: 40) == 0)
    }

    @Test("размеры в заголовке растут вместе с данными")
    func headerTracksWrittenBytes() throws {
        let url = tempURL()
        defer { try? FileManager.default.removeItem(at: url) }
        let w = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
        let payload = Data(repeating: 0xAB, count: 1_000)
        try w.appendBytes(payload)
        try w.close()

        let bytes = try Data(contentsOf: url)
        #expect(u32le(bytes, at: 40) == 1_000)
        #expect(u32le(bytes, at: 4) == 1_036)
        #expect(bytes.count == 1_044)
    }

    @Test("запись сверх потолка отклоняется, а не заворачивается")
    func writeBeyondLimitIsRejected() throws {
        // Регрессия TD-21c: счётчик был `UInt32` с `&+=`, поэтому после 4 GiB
        // он молча заворачивался и в заголовок уезжал мусор — файл переставал
        // открываться целиком. Настоящие 4 GiB здесь не пишем: подводим
        // счётчик к границе и проверяем поведение на ней.
        let url = tempURL()
        defer { try? FileManager.default.removeItem(at: url) }
        let w = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
        w.primeDataBytesForTesting(WAVWriter.maxDataBytes - 4)

        try w.appendBytes(Data(repeating: 0, count: 4))  // ровно до потолка
        #expect(!w.didHitLimit, "точное попадание в потолок — ещё не превышение")

        #expect(throws: WAVWriterError.sizeLimitReached) {
            try w.appendBytes(Data(repeating: 0, count: 1))
        }
        #expect(w.didHitLimit, "флаг обязан подняться — деградация видима")
    }

    @Test("после потолка write(buffer:) возвращает 0, а не врёт о записанном")
    func writeBufferReturnsZeroAfterLimit() throws {
        let url = tempURL()
        defer { try? FileManager.default.removeItem(at: url) }
        let w = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
        w.primeDataBytesForTesting(WAVWriter.maxDataBytes)

        let buffer = makeInt16Buffer(frames: 128)
        let written = try w.write(buffer: buffer)
        #expect(written == 0, "счётчик байт звонка не должен расти впустую")
        #expect(w.didHitLimit)
    }

    @Test("заголовок остаётся валидным даже когда потолок достигнут")
    func headerStaysValidAtLimit() throws {
        let url = tempURL()
        defer { try? FileManager.default.removeItem(at: url) }
        let w = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
        w.primeDataBytesForTesting(WAVWriter.maxDataBytes)
        try w.close()

        let bytes = try Data(contentsOf: url)
        // Сужение до UInt32 обязано быть точным, без обрезки: raw + 36
        // помещается в UInt32 ровно потому, что потолок это учитывает.
        #expect(u32le(bytes, at: 40) == UInt32(WAVWriter.maxDataBytes))
        #expect(u32le(bytes, at: 4) == UInt32(WAVWriter.maxDataBytes) + 36)
    }

    // MARK: - helpers

    private func u32le(_ data: Data, at offset: Int) -> UInt32 {
        let slice = data[data.startIndex + offset..<data.startIndex + offset + 4]
        return slice.reversed().reduce(UInt32(0)) { ($0 << 8) | UInt32($1) }
    }

    private func makeInt16Buffer(frames: AVAudioFrameCount) -> AVAudioPCMBuffer {
        let format = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: 16_000,
            channels: 1,
            interleaved: true
        )!
        let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames)!
        buffer.frameLength = frames
        return buffer
    }
}

@Suite("WAVWriter — тишина в провале (TD-45)")
struct WAVWriterSilenceTests {
    private func tmpURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("wotold-silence-\(UUID().uuidString).wav")
    }

    @Test("длительность тишины считается по частоте и каналам")
    func silenceLengthMatchesSampleRate() throws {
        let url = tmpURL()
        let w = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
        defer { try? FileManager.default.removeItem(at: url) }

        let written = try w.writeSilence(seconds: 1.5)
        // 16 кГц × 1 канал × 2 байта × 1.5 с
        #expect(written == 48_000)
        try w.close()

        let size = try FileManager.default
            .attributesOfItem(atPath: url.path)[.size] as? Int ?? 0
        #expect(size == 44 + 48_000, "заголовок 44 байта + данные")
    }

    @Test("тишина — действительно нули, а не мусор из памяти")
    func silenceIsActuallySilent() throws {
        let url = tmpURL()
        let w = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
        defer { try? FileManager.default.removeItem(at: url) }
        try w.writeSilence(seconds: 0.05)
        try w.close()

        let bytes = try Data(contentsOf: url).dropFirst(44)
        #expect(!bytes.isEmpty)
        #expect(bytes.allSatisfy { $0 == 0 })
    }

    @Test("нулевая и бессмысленная длительность ничего не пишут")
    func nonPositiveDurationsAreNoOps() throws {
        let url = tmpURL()
        let w = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
        defer { try? FileManager.default.removeItem(at: url) }

        #expect(try w.writeSilence(seconds: 0) == 0)
        #expect(try w.writeSilence(seconds: -3) == 0)
        #expect(try w.writeSilence(seconds: .nan) == 0)
        #expect(try w.writeSilence(seconds: .infinity) == 0)
        try w.close()

        let size = try FileManager.default
            .attributesOfItem(atPath: url.path)[.size] as? Int ?? 0
        #expect(size == 44, "только заголовок")
    }

    @Test("длинный провал пишется чанками и не врёт про объём")
    func longGapIsWrittenInChunks() throws {
        let url = tmpURL()
        let w = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
        defer { try? FileManager.default.removeItem(at: url) }
        // 10 с — заведомо больше одного 16 KiB-чанка.
        let written = try w.writeSilence(seconds: 10)
        #expect(written == 320_000)
        try w.close()
    }
}

