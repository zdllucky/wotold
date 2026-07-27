import AVFoundation
import CoreAudio
import Foundation
import WotoldAudioCore

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
    // [TD-21] Синхронизация — как у близнеца AudioRecorder (правило 2).
    private let level = AtomicLevel()
    private var isPaused = false

    // [TD-44] Обнаружение потери системной дорожки. Механизм близнеца
    // (TD-21e) сюда не переносится дословно: `AVAudioEngine` здесь нет, а
    // значит нет и `AVAudioEngineConfigurationChange`. Аналог — слушатель
    // свойства Core Audio на смену устройства вывода как быстрый триггер
    // плюс тот же самый детектор тишины, который про микрофон ничего не
    // знает. Решение «сообщать пользователю» принимает детектор, а не
    // слушатель: свойство меняется и без потери кадров.
    private var stallDetector = AudioStallDetector()
    private var stallTimer: DispatchSourceTimer?
    private let stallTickInterval: TimeInterval = 1.0
    private var defaultOutputListener: AudioObjectPropertyListenerBlock?
    /// Удался ли последний пересбор цепочки tap → aggregate → IOProc. Идёт в
    /// `device_recovered.restarted`: дорожка могла вернуться и сама.
    private var lastRebuildSucceeded = false
    /// Идёт остановка — пересобирать захват нельзя (см. близнеца).
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
            // [TD-44] Детектор обязан знать про паузу: на паузе кадров нет по
            // замыслу, и без этого TD-07 выглядела бы как отвалившееся
            // устройство (близнец делает то же самое).
            self.stallDetector.setPaused(paused, at: self.now())
        }
    }

    private func now() -> TimeInterval { ProcessInfo.processInfo.systemUptime }

    func start(systemURL: URL) async throws {
        // [TD-21b] Повторный start() без остановки предыдущего перезаписывал
        // tapID/aggregateID/ioProcID — прежние Core Audio объекты оставались
        // жить до выхода процесса, а IOProc продолжал стучаться в already-
        // replaced writer. Близнец `AudioRecorder.start` такую защиту имел
        // с самого начала (правило 2).
        if aggregateID != kAudioObjectUnknown || tapID != kAudioObjectUnknown {
            _ = try? await stop()
        }

        guard let outFormat = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: 16_000,
            channels: 1,
            interleaved: true
        ) else {
            throw Self.error("failed to create output format (16kHz mono i16)")
        }

        // 1-5. Tap + aggregate device.
        let chain = try Self.makeTapChain()

        // 6. Открыть WAV writer.
        let writer: WAVWriter
        do {
            writer = try WAVWriter(url: systemURL, sampleRate: 16_000, channels: 1)
        } catch {
            Self.destroyChain(aggregateID: chain.aggregateID, tapID: chain.tapID)
            throw error
        }

        // 7-8. IOProc поверх aggregate + старт IO.
        let validProcID: AudioDeviceIOProcID
        do {
            validProcID = try attachIOProc(aggregateID: chain.aggregateID)
        } catch {
            try? writer.close()
            Self.destroyChain(aggregateID: chain.aggregateID, tapID: chain.tapID)
            throw error
        }

        // Стор всех ресурсов в инстансе.
        self.tapID = chain.tapID
        self.aggregateID = chain.aggregateID
        self.ioProcID = validProcID
        self.wavWriter = writer
        self.outputFormat = outFormat
        self.inputFormat = chain.inFormat
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

        // [TD-44] Наблюдение за живостью дорожки — после того как всё
        // поднялось, иначе первый же тик увидел бы тишину.
        startStallWatchdog()
        observeDefaultOutputDevice()
    }

    // MARK: - Сборка цепочки tap → aggregate → IOProc

    /// Всё, что нужно, чтобы кадры пошли: tap, aggregate-устройство поверх
    /// него и формат входа. Выделено из `start`, потому что при потере
    /// устройства ту же цепочку надо собрать заново, не трогая WAV-writer.
    private struct TapChain {
        let tapID: AudioObjectID
        let aggregateID: AudioObjectID
        let inFormat: AVAudioFormat
    }

    private static func makeTapChain() throws -> TapChain {
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

        return TapChain(tapID: newTapID, aggregateID: newAggregateID, inFormat: inFormat)
    }

    private static func destroyChain(aggregateID: AudioObjectID, tapID: AudioObjectID) {
        if aggregateID != kAudioObjectUnknown {
            AudioHardwareDestroyAggregateDevice(aggregateID)
        }
        if tapID != kAudioObjectUnknown {
            AudioHardwareDestroyProcessTap(tapID)
        }
    }

    /// Повесить IOProc на aggregate и запустить IO. WAV-writer сюда не
    /// передаётся намеренно: за его закрытие отвечает вызывающий, который
    /// знает, создавал он writer или переиспользует существующий.
    private func attachIOProc(aggregateID: AudioObjectID) throws -> AudioDeviceIOProcID {
        var procID: AudioDeviceIOProcID?
        var status = AudioDeviceCreateIOProcIDWithBlock(&procID, aggregateID, queue) {
            [weak self] _, inInputData, _, _, _ in
            self?.handleAudio(inputData: inInputData)
        }
        guard status == noErr, let validProcID = procID else {
            throw Self.error("AudioDeviceCreateIOProcIDWithBlock failed (\(status))")
        }
        status = AudioDeviceStart(aggregateID, validProcID)
        guard status == noErr else {
            AudioDeviceDestroyIOProcID(aggregateID, validProcID)
            throw Self.error("AudioDeviceStart failed (\(status))")
        }
        return validProcID
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
            // [TD-06] См. AudioRecorder.rotate: обнуляем до открытия, чтобы при
            // провале не писать в закрытый handle и дать следующей ротации
            // поднять системную дорожку заново.
            wavWriter = nil

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
        // [TD-44] Гасим наблюдение ДО остановки IO: иначе тишина после
        // `AudioDeviceStop` выглядела бы как потеря устройства, и юзер
        // получал бы `device_lost` на каждой нормальной остановке.
        stallTimer?.cancel()
        stallTimer = nil
        removeDefaultOutputObserver()
        queue.sync { self.isStopping = true }

        // [TD-21a] Сначала глушим источник кадров, потом закрываем файл — и
        // закрываем его НА `queue`, а не на вызывающем потоке.
        //
        // IOProc создан через `AudioDeviceCreateIOProcIDWithBlock(..., queue)`,
        // то есть `handleAudio` исполняется на `queue`. `AudioDeviceStop`
        // прекращает подачу новых кадров, но уже поставленный в очередь блок
        // ещё может выполниться. Раньше `close()` и обнуление полей шли на
        // вызывающем потоке — то есть параллельно с этим блоком: запись в
        // закрытый FileHandle и потеря хвоста последнего system-чанка.
        //
        // Это ровно та гонка, которую в близнеце `AudioRecorder.stop`
        // починили ещё в M13 (`queue.sync` вокруг close+nil). Здесь она
        // оставалась открытой — «одинаковый контракт, разная зрелость»
        // (правило 2).
        if aggregateID != kAudioObjectUnknown, let procID = ioProcID {
            _ = AudioDeviceStop(aggregateID, procID)
            _ = AudioDeviceDestroyIOProcID(aggregateID, procID)
        }

        let bytes: UInt64 = try queue.sync {
            try wavWriter?.close()
            let b = bytesWritten
            wavWriter = nil
            converter = nil
            outputFormat = nil
            inputFormat = nil
            bytesWritten = 0
            level.reset()
            return b
        }

        // Разрушение Core Audio объектов — уже после того, как обработчик
        // гарантированно не выполняется: `queue.sync` выше дождался всех
        // ранее поставленных блоков.
        if aggregateID != kAudioObjectUnknown {
            _ = AudioHardwareDestroyAggregateDevice(aggregateID)
        }
        if tapID != kAudioObjectUnknown {
            _ = AudioHardwareDestroyProcessTap(tapID)
        }
        aggregateID = kAudioObjectUnknown
        tapID = kAudioObjectUnknown
        ioProcID = nil

        return StopResult(bytesWritten: bytes)
    }

    // MARK: - [TD-44] Потеря устройства

    /// Watchdog поверх детектора тишины. Тот же приём, что у близнеца:
    /// решение принимает детектор, таймер лишь тикает.
    private func startStallWatchdog() {
        stallTimer?.cancel()
        // Детектор читается и пишется обработчиком таймера и IOProc'ом — оба
        // на `queue`. Инициализация обязана идти там же: `start()` исполняется
        // на потоке вызывающего, и без sync это гонка со стартом записи.
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
            // Слушатель свойства мог не сработать вовсе (tap отвалился без
            // смены устройства вывода) — пробуем поднять дорожку сами.
            self.rebuildTapChain()
        }
        t.resume()
        stallTimer = t
    }

    /// Смена устройства вывода — быстрый триггер пересбора. Сообщение
    /// пользователю решает не она: устройство может смениться без единого
    /// потерянного кадра, и говорить о потере было бы враньём.
    private func observeDefaultOutputDevice() {
        removeDefaultOutputObserver()
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let block: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
            self?.queue.async { self?.rebuildTapChain() }
        }
        let status = AudioObjectAddPropertyListenerBlock(
            AudioObjectID(kAudioObjectSystemObject), &address, queue, block
        )
        if status == noErr {
            defaultOutputListener = block
        } else {
            FileHandle.standardError.write(
                Data("system default-output listener failed (\(status))\n".utf8)
            )
        }
    }

    private func removeDefaultOutputObserver() {
        guard let block = defaultOutputListener else { return }
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        _ = AudioObjectRemovePropertyListenerBlock(
            AudioObjectID(kAudioObjectSystemObject), &address, queue, block
        )
        defaultOutputListener = nil
    }

    /// Пересобрать tap → aggregate → IOProc, не трогая WAV-writer: файл и
    /// накопленные байты те же, меняется только источник кадров.
    ///
    /// Вызывается с двух сторон (слушатель свойства и watchdog) и обязан быть
    /// идемпотентным: пока кадры идут, трогать ничего нельзя — пересбор сам
    /// по себе роняет несколько десятков миллисекунд звука.
    private func rebuildTapChain() {
        guard !isStopping, wavWriter != nil else { return }
        // Кадры идут — значит дорожка жива, и повод не наш.
        if !stallDetector.stalled { return }

        if aggregateID != kAudioObjectUnknown, let procID = ioProcID {
            _ = AudioDeviceStop(aggregateID, procID)
            _ = AudioDeviceDestroyIOProcID(aggregateID, procID)
        }
        Self.destroyChain(aggregateID: aggregateID, tapID: tapID)
        aggregateID = kAudioObjectUnknown
        tapID = kAudioObjectUnknown
        ioProcID = nil

        do {
            let chain = try Self.makeTapChain()
            let procID = try attachIOProc(aggregateID: chain.aggregateID)
            tapID = chain.tapID
            aggregateID = chain.aggregateID
            ioProcID = procID
            inputFormat = chain.inFormat
            // Новый tap — потенциально новый входной формат. Конвертер
            // построен под старый; переиспользовать его значило бы писать
            // мусор в WAV.
            converter = nil
            lastRebuildSucceeded = true
        } catch {
            lastRebuildSucceeded = false
            FileHandle.standardError.write(
                Data("system tap rebuild failed: \(error.localizedDescription)\n".utf8)
            )
        }
    }

    /// [TD-45] Записать тишину длиной провала в текущий WAV. Ошибка записи не
    /// фатальна: дорожка уже пострадала, ронять из-за выравнивания весь
    /// захват — хуже.
    private func padGapWithSilence(_ gapSec: TimeInterval) {
        guard gapSec > 0, let writer = wavWriter else { return }
        do {
            let written = try writer.writeSilence(seconds: gapSec)
            if written > 0 {
                bytesWritten &+= UInt64(written)
                FileHandle.standardError.write(
                    Data("system track: дописано \(String(format: "%.1f", gapSec)) с тишины в провал\n".utf8)
                )
            }
        } catch {
            FileHandle.standardError.write(
                Data("system track: не удалось выровнять провал: \(error.localizedDescription)\n".utf8)
            )
        }
    }

    private func report(_ event: AudioStallDetector.Event) {
        switch event {
        case let .lost(since):
            lastRebuildSucceeded = false
            // Индикатор обязан упасть в ноль: иначе он замирает на последнем
            // значении и показывает живой сигнал у мёртвой дорожки.
            level.reset()
            onDeviceEvent?(
                .deviceLost(
                    leg: .system,
                    message: DeviceEventText.lost(leg: .system, silentSec: now() - since)
                ))
        case let .recovered(gapSec):
            // [TD-45] Дыру заполняем тишиной ДО того, как в файл пойдут новые
            // кадры. Дорожки сливаются по таймкодам внутри каждого WAV, и без
            // этого весь остаток дорожки уезжает относительно второй ровно на
            // длительность провала. Решение владельца: дописывать тишину.
            // Фабрикация содержимого — поэтому событие ниже обязано дойти до
            // UI (degraded-флаг, TD-37), а не остаться в логе.
            padGapWithSilence(gapSec)
            onDeviceEvent?(
                .deviceRecovered(leg: .system, gapSec: gapSec, restarted: lastRebuildSucceeded))
            lastRebuildSucceeded = false
        }
    }

    // MARK: - IOProc

    private func handleAudio(inputData: UnsafePointer<AudioBufferList>) {
        // [TD-44] Кадр пришёл — до всех проверок. Дорожка жива даже когда мы
        // этот кадр дропаем (пауза, битый формат): «замолчала» означает, что
        // Core Audio перестал звать обработчик, а не что мы ничего не пишем.
        if let event = stallDetector.frameArrived(at: now()) {
            report(event)
        }
        // [TD-07] См. AudioRecorder.processBuffer — системная дорожка на паузе
        // тоже не пишется, иначе собеседника было бы слышно в «приватной» части.
        if isPaused {
            level.reset()
            return
        }
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

        level.set(computeInt16Rms(outBuffer))
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
