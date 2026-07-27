import Foundation

/// [TD-21e] Обнаружение «дорожка замолчала» по факту прекращения кадров.
///
/// Почему не по `AVAudioEngineConfigurationChange` и не по `engine.isRunning`.
/// Замерено живьём на этой машине (macOS, AirPods Max ↔ встроенный микрофон,
/// смена дефолтного входа во время записи):
///
/// - смена устройства **молча** останавливает подачу кадров: ошибки нет,
///   исключения нет, счётчик просто перестаёт расти;
/// - `engine.isRunning` при этом до нескольких секунд возвращает `true`,
///   то есть как признак живости он врёт;
/// - уведомление о смене конфигурации приходит, но с разной задержкой:
///   ~0.5 с при переключении на встроенный микрофон и **~3.5 с** при
///   возврате на Bluetooth. Всё это время дорожка мертва.
///
/// Единственный честный признак — «кадры перестали приходить». Уведомление
/// остаётся полезным как быстрый триггер для восстановления, но решение
/// «сообщать пользователю» принимается здесь.
///
/// Тип чистый, время инжектируется: тесты не спят (правило 6).
public struct AudioStallDetector: Equatable {
    /// Сколько тишины считать сбоем.
    ///
    /// 3 секунды — компромисс по замерам: восстановление после переключения
    /// на проводное устройство укладывается в ~0.5 с, и сообщать о нём
    /// значило бы шуметь на каждом мелком переключении. Bluetooth-случай
    /// занимает ~5 с и обязан быть виден. Порог между ними, ближе к нижней
    /// границе — потерянная секунда речи дороже лишней строки в логе.
    public static let defaultStallThreshold: TimeInterval = 3.0

    /// Что произошло — если произошло.
    public enum Event: Equatable {
        /// Дорожка замолчала. `since` — момент последнего кадра.
        case lost(since: TimeInterval)
        /// Кадры вернулись. `gapSec` — длительность провала.
        case recovered(gapSec: TimeInterval)
    }

    private let threshold: TimeInterval
    private var lastFrameAt: TimeInterval?
    private var isStalled = false
    private var isPaused = false

    public init(threshold: TimeInterval = AudioStallDetector.defaultStallThreshold) {
        self.threshold = threshold
    }

    /// Запись стартовала — с этого момента ждём кадры.
    public mutating func started(at now: TimeInterval) {
        lastFrameAt = now
        isStalled = false
        isPaused = false
    }

    /// Пришёл кадр. Возвращает `.recovered`, если это первый кадр после сбоя.
    public mutating func frameArrived(at now: TimeInterval) -> Event? {
        let previous = lastFrameAt
        lastFrameAt = now
        guard isStalled else { return nil }
        isStalled = false
        let gap = previous.map { max(0, now - $0) } ?? 0
        return .recovered(gapSec: gap)
    }

    /// Тик наблюдателя. Возвращает `.lost` ровно один раз на сбой.
    public mutating func tick(at now: TimeInterval) -> Event? {
        // На паузе кадров нет по замыслу — это не сбой. Именно поэтому
        // детектор обязан знать про паузу: иначе TD-07 (пауза на уровне
        // захвата) выглядела бы как отвалившееся устройство.
        guard !isPaused, !isStalled, let last = lastFrameAt else { return nil }
        guard now - last >= threshold else { return nil }
        isStalled = true
        return .lost(since: last)
    }

    /// Пауза/возобновление захвата.
    public mutating func setPaused(_ paused: Bool, at now: TimeInterval) {
        isPaused = paused
        if !paused {
            // После возобновления отсчёт тишины начинается заново, иначе
            // длинная пауза мгновенно выглядела бы как сбой.
            lastFrameAt = now
            isStalled = false
        }
    }

    /// Сейчас в состоянии сбоя.
    public var stalled: Bool { isStalled }
}
