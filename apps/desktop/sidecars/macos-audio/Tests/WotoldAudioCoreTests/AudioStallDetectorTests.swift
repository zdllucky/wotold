import Foundation
import Testing

@testable import WotoldAudioCore

/// [TD-21e] Время инжектируется — тесты не спят (правило 6).
@Suite("AudioStallDetector (TD-21e)")
struct AudioStallDetectorTests {

    @Test("пока кадры идут, сбоя нет")
    func steadyFramesNeverStall() {
        var d = AudioStallDetector()
        d.started(at: 0)
        for t in stride(from: 0.5, through: 20.0, by: 0.5) {
            #expect(d.frameArrived(at: t) == nil)
            #expect(d.tick(at: t) == nil)
        }
        #expect(!d.stalled)
    }

    @Test("тишина дольше порога — ровно одно сообщение о сбое")
    func stallReportedOnce() {
        var d = AudioStallDetector(threshold: 3.0)
        d.started(at: 10)
        _ = d.frameArrived(at: 10)

        #expect(d.tick(at: 12.9) == nil, "до порога — молчим")
        #expect(d.tick(at: 13.0) == .lost(since: 10))
        // Повторные тики не должны спамить: событие однократно на сбой.
        #expect(d.tick(at: 14.0) == nil)
        #expect(d.tick(at: 30.0) == nil)
        #expect(d.stalled)
    }

    @Test("возвращение кадров даёт длительность провала")
    func recoveryReportsGap() {
        var d = AudioStallDetector(threshold: 3.0)
        d.started(at: 0)
        _ = d.frameArrived(at: 1.0)
        #expect(d.tick(at: 4.0) == .lost(since: 1.0))

        // Замеренный Bluetooth-случай: провал около пяти секунд.
        #expect(d.frameArrived(at: 6.2) == .recovered(gapSec: 5.2))
        #expect(!d.stalled)
        // Дальше — тишина в эфире, а не поток событий.
        #expect(d.frameArrived(at: 6.7) == nil)
    }

    @Test("пауза не считается сбоем")
    func pauseIsNotAStall() {
        // Регрессия: TD-07 останавливает кадры намеренно. Без учёта паузы
        // каждая пауза дольше порога выглядела бы как отвалившееся
        // устройство, и пользователь получал бы ложную тревогу ровно тогда,
        // когда сам нажал «пауза».
        var d = AudioStallDetector(threshold: 3.0)
        d.started(at: 0)
        _ = d.frameArrived(at: 1.0)
        d.setPaused(true, at: 1.5)
        for t in stride(from: 2.0, through: 60.0, by: 1.0) {
            #expect(d.tick(at: t) == nil, "на паузе сбоя быть не может (t=\(t))")
        }
        #expect(!d.stalled)
    }

    @Test("после снятия паузы отсчёт начинается заново")
    func resumeRestartsTheClock() {
        var d = AudioStallDetector(threshold: 3.0)
        d.started(at: 0)
        _ = d.frameArrived(at: 1.0)
        d.setPaused(true, at: 1.5)
        d.setPaused(false, at: 100.0)
        // Иначе 98 секунд паузы мгновенно дали бы «сбой» на первом же тике.
        #expect(d.tick(at: 101.0) == nil)
        #expect(d.tick(at: 103.0) == .lost(since: 100.0))
    }

    @Test("второй сбой после восстановления снова сообщается")
    func secondStallIsReported() {
        var d = AudioStallDetector(threshold: 3.0)
        d.started(at: 0)
        _ = d.frameArrived(at: 1.0)
        #expect(d.tick(at: 4.0) == .lost(since: 1.0))
        #expect(d.frameArrived(at: 5.0) == .recovered(gapSec: 4.0))
        #expect(d.tick(at: 8.0) == .lost(since: 5.0))
    }

    @Test("до первого кадра сбой не объявляется")
    func noStallBeforeAnyFrame() {
        // start() ставит отсчёт, но если кадров не было вовсе — это отдельная
        // история (не поднялся захват), и она обязана всплыть как ошибка
        // старта, а не как «устройство пропало».
        var d = AudioStallDetector(threshold: 3.0)
        #expect(d.tick(at: 100.0) == nil, "без started() детектор молчит")
    }
}
