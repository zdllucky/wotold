import AVFoundation
import Foundation
import WotoldAudioCore

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
    // [TD-21] Пишется из обработчика кадров, читается с таймерной очереди —
    // раньше без всякой синхронизации. См. AtomicLevel.
    private let level = AtomicLevel()
    private var isPaused = false

    // [TD-21e] Обнаружение и восстановление потери устройства.
    private var stallDetector = AudioStallDetector()
    private var stallTimer: DispatchSourceTimer?
    private var configObserver: NSObjectProtocol?
    private var micURL: URL?
    private let stallTickInterval: TimeInterval = 1.0
    /// Удался ли последний пересбор. Идёт в `device_recovered.restarted`,
    /// чтобы событие не утверждало о перезапуске, которого не было: дорожка
    /// могла вернуться и сама, когда устройство появилось обратно.
    private var lastRebuildSucceeded = false
    /// Идёт остановка — пересобирать захват нельзя. Ставится на `queue`,
    /// читается там же: иначе пересбор, начатый за миг до `stop()`, поднял бы
    /// новый engine уже после того, как стоп решил, что всё погашено.
    private var isStopping = false

    /// Куда сообщать о потере и возврате дорожки. Ставится извне (App.swift),
    /// чтобы рекордер не знал про stdout и протокол.
    var onDeviceEvent: ((SidecarEvent) -> Void)?

    var currentRms: Float { level.value }

    /// [TD-07] Пауза на уровне ЗАХВАТА: кадры дропаются до записи в WAV.
    /// До этого пауза жила только в БД, и сказанное «на паузе» попадало в
    /// файл, транскрипт и саммари.
    ///
    /// Флаг читается и пишется на той же serial queue, что и обработчик
    /// кадров, поэтому отдельная синхронизация не нужна.
    func setPaused(_ paused: Bool) {
        queue.sync {
            self.isPaused = paused
            // [TD-21e] Пауза — не сбой устройства: детектор обязан знать.
            self.stallDetector.setPaused(paused, at: self.now())
        }
    }

    func start(micURL: URL) throws {
        // Если уже пишем — стопаем предыдущий чтобы не потерять состояние.
        if engine != nil {
            _ = try? stop()
        }

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

        let writer = try WAVWriter(url: micURL, sampleRate: 16_000, channels: 1)

        self.micURL = micURL
        self.outputFormat = outFormat
        self.wavWriter = writer
        self.startTime = Date()
        self.bytesWritten = 0

        try buildEngine()

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

        startStallWatchdog()
    }

    /// [TD-21e] Поднять НОВЫЙ `AVAudioEngine` под текущее устройство ввода.
    ///
    /// Используется и при старте, и при восстановлении после потери
    /// устройства. `wavWriter` намеренно не трогается: чанк продолжается тем
    /// же файлом, в нём просто окажется дыра длиной в провал.
    ///
    /// Почему именно пересборка с нуля, а не что-то дешевле — замерено живьём
    /// (AirPods Max ↔ встроенный микрофон, смена дефолтного входа на ходу):
    /// - переустановка tap'а на существующем engine кидает ObjC-исключение из
    ///   `AVAudioEngineGraph::InstallTapOnNode`, а его из Swift не поймать —
    ///   сайдкар просто падает, унося всю запись;
    /// - `engine.start()` без переустановки tap'а возвращает `-10868`
    ///   (`FormatNotSupported`), потому что формат tap'а прибит на момент
    ///   установки, а железо меняет частоту (замерено 24000 → 48000 Гц);
    /// - новый engine поднимается штатно и кадры идут через ~0.5 с.
    private func buildEngine() throws {
        guard let outFormat = self.outputFormat else {
            throw NSError(
                domain: "WotoldAudio",
                code: 3,
                userInfo: [NSLocalizedDescriptionKey: "buildEngine before start"]
            )
        }
        let engine = AVAudioEngine()
        let input = engine.inputNode
        let inFormat = input.outputFormat(forBus: 0)
        guard inFormat.sampleRate > 0, inFormat.channelCount > 0 else {
            throw NSError(
                domain: "WotoldAudio",
                code: 4,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "input device has no usable format (\(inFormat.sampleRate) Hz,"
                        + " \(inFormat.channelCount) ch)"
                ]
            )
        }
        // Конвертер обязателен к пересозданию: частота железа меняется вместе
        // с устройством, а старый конвертер настроен на прежний вход.
        guard let conv = AVAudioConverter(from: inFormat, to: outFormat) else {
            throw NSError(
                domain: "WotoldAudio",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "failed to create AVAudioConverter"]
            )
        }

        // [M13] processBuffer читает writer/converter/outFormat из self.*, а не
        // из captured params — это позволяет атомарно swap'нуть self.wavWriter
        // в rotate(to:) без замены закрытого tap'а.
        input.installTap(onBus: 0, bufferSize: 4096, format: inFormat) {
            [weak self] inBuffer, _ in
            guard let self else { return }
            self.queue.async { [weak self] in
                self?.processBuffer(inBuffer)
            }
        }

        try engine.start()

        self.engine = engine
        self.converter = conv
        self.inputFormat = inFormat
        observeConfigurationChange(of: engine)
    }

    // MARK: - [TD-21e] Потеря устройства

    private func now() -> TimeInterval { ProcessInfo.processInfo.systemUptime }

    /// Подписка на смену конфигурации — быстрый триггер восстановления.
    /// Сообщение пользователю решает не она, а watchdog: уведомление приходит
    /// с задержкой до 3.5 с (замерено на возврате Bluetooth-устройства), и всё
    /// это время `engine.isRunning` возвращает `true` при нуле кадров.
    private func observeConfigurationChange(of engine: AVAudioEngine) {
        if let old = configObserver {
            NotificationCenter.default.removeObserver(old)
        }
        configObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange,
            object: engine,
            queue: nil
        ) { [weak self] _ in
            // Уведомление приходит на произвольном потоке; состояние живёт
            // на `queue`. `async`, а не `sync` — иначе рискуем самоблокировкой,
            // если уведомление придёт на самой очереди.
            self?.queue.async { [weak self] in
                self?.rebuildAfterConfigurationChange()
            }
        }
    }

    private func rebuildAfterConfigurationChange() {
        guard !isStopping, engine != nil, wavWriter != nil else { return }
        // Уведомления приходят пачками (замерено три подряд за секунду).
        // Работающий engine — не повод что-то трогать.
        if engine?.isRunning == true { return }
        engine?.stop()
        do {
            try buildEngine()
            lastRebuildSucceeded = true
        } catch {
            lastRebuildSucceeded = false
            FileHandle.standardError.write(
                Data("mic engine rebuild failed: \(error.localizedDescription)\n".utf8)
            )
        }
    }

    private func startStallWatchdog() {
        stallTimer?.cancel()
        // Детектор читается и пишется обработчиком таймера на `queue`.
        // Инициализация обязана идти там же, иначе это гонка со стартом
        // записи (start() исполняется на потоке вызывающего).
        queue.sync {
            self.stallDetector = AudioStallDetector()
            self.stallDetector.started(at: self.now())
            self.isStopping = false
        }
        let t = DispatchSource.makeTimerSource(queue: queue)
        t.schedule(deadline: .now() + stallTickInterval, repeating: stallTickInterval)
        t.setEventHandler { [weak self] in
            guard let self else { return }
            guard let event = self.stallDetector.tick(at: self.now()) else { return }
            self.report(event)
            // Уведомление могло не прийти вовсе — пробуем поднять дорожку сами.
            self.rebuildAfterConfigurationChange()
        }
        t.resume()
        stallTimer = t
    }

    private func report(_ event: AudioStallDetector.Event) {
        switch event {
        case let .lost(since):
            lastRebuildSucceeded = false
            // Индикатор обязан упасть в ноль: иначе он замирает на последнем
            // значении речи и показывает живой сигнал у мёртвой дорожки.
            level.reset()
            let silent = now() - since
            onDeviceEvent?(
                .deviceLost(
                    leg: .mic,
                    message: "микрофонная дорожка молчит \(String(format: "%.1f", silent)) с — "
                        + "устройство ввода пропало или сменилось"
                ))
        case let .recovered(gapSec):
            onDeviceEvent?(
                .deviceRecovered(leg: .mic, gapSec: gapSec, restarted: lastRebuildSucceeded))
            lastRebuildSucceeded = false
        }
    }

    private func processBuffer(_ inBuffer: AVAudioPCMBuffer) {
        // [TD-07] На паузе кадр не доходит до WAV. RMS обнуляем: индикатор
        // уровня должен показывать, что звук НЕ пишется, а не замирать на
        // последнем значении — для privacy-фичи это часть контракта.
        if isPaused {
            level.reset()
            return
        }
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

        // [TD-21e] Живость отмечается ЗДЕСЬ, после успешной конвертации, а не
        // на входе в обработчик. Разница принципиальна: `AVAudioConverter`
        // прибит к формату входа на момент создания, и при смене устройства
        // конвертация может проваливаться на КАЖДОМ кадре. Тогда кадры
        // «идут», а в файл не попадает ничего — отмечай мы живость по факту
        // прихода, watchdog считал бы дорожку здоровой, пока она пишет тишину.
        if let event = stallDetector.frameArrived(at: now()) {
            report(event)
        }

        do {
            let bytes = try writer.write(buffer: outBuffer)
            bytesWritten += UInt64(bytes)
        } catch {
            FileHandle.standardError.write(
                Data("wav write error: \(error.localizedDescription)\n".utf8)
            )
        }

        // [B14] RMS post-write — frontend читает latestRms через эмит таймер.
        level.set(computeInt16Rms(outBuffer))
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
            // [TD-06] Обнуляем ДО открытия нового: если WAVWriter бросит, в
            // self.wavWriter иначе останется уже закрытый writer, и каждый
            // следующий кадр будет уходить в него (write-after-close). С nil
            // processBuffer просто пропускает кадры, а следующая ротация
            // поднимет дорожку заново.
            self.wavWriter = nil

            // Open new chunk WAV — same format (16kHz mono i16).
            let newWriter = try WAVWriter(url: url, sampleRate: 16_000, channels: 1)
            self.wavWriter = newWriter
            self.startTime = Date()
            self.bytesWritten = 0
            return (durationSec: oldDuration, micBytes: oldBytes)
        }
    }

    func stop() throws -> (durationSec: Double, micBytes: UInt64) {
        // [TD-21e] Флаг ставим первым и на `queue`: он же дожидается
        // пересбора, который мог начаться мгновением раньше.
        queue.sync { self.isStopping = true }
        flushTimer?.cancel()
        flushTimer = nil
        // [TD-21e] Снимаем наблюдение ДО остановки engine, иначе штатный стоп
        // сам породит смену конфигурации и обработчик полезет пересобирать
        // уже останавливаемую запись.
        stallTimer?.cancel()
        stallTimer = nil
        if let o = configObserver {
            NotificationCenter.default.removeObserver(o)
            configObserver = nil
        }

        // [M13 fix] Останавливаем tap ПЕРВЫМ (нет новых callback'ов), затем
        // close()+nil делаем на `queue` через sync — как в rotate(). Иначе
        // close() на calling-thread'е гонится с уже-dispatch'нутыми
        // processBuffer'ами на queue (concurrent write/close одного FileHandle)
        // + последние 1-2 буфера теряются → финальный chunk WAV обрезан/битый
        // (тот самый файл, который M13 final-chunk шаг читает).
        engine?.inputNode.removeTap(onBus: 0)
        engine?.stop()

        let (duration, bytes): (Double, UInt64) = try queue.sync { [weak self] in
            guard let self = self else { return (0, 0) }
            try self.wavWriter?.close()
            let d = self.startTime.map { Date().timeIntervalSince($0) } ?? 0
            let b = self.bytesWritten
            self.wavWriter = nil
            return (d, b)
        }

        engine = nil
        converter = nil
        outputFormat = nil
        inputFormat = nil
        startTime = nil
        bytesWritten = 0
        micURL = nil
        level.reset()

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
