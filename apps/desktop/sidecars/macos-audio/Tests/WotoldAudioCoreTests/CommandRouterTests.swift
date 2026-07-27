import Foundation
import Testing

@testable import WotoldAudioCore

// [TD-06] Роутер тестируется моками — без Core Audio, микрофона и разрешений.
// Именно эти сценарии раньше проверялись только живым звонком.

private struct Boom: Error, LocalizedError {
    let what: String
    var errorDescription: String? { what }
}

private final class MockMic: MicRecording {
    var startError: Error?
    var rotateError: Error?
    var stopError: Error?
    private(set) var started = false
    private(set) var stopped = false
    private(set) var rotatedTo: [String] = []
    private(set) var pauseCalls: [Bool] = []

    func setPaused(_ paused: Bool) { pauseCalls.append(paused) }

    func start(micURL: URL) throws {
        if let startError { throw startError }
        started = true
    }
    func rotate(to url: URL) throws -> (durationSec: Double, micBytes: UInt64) {
        if let rotateError { throw rotateError }
        rotatedTo.append(url.path)
        return (durationSec: 600, micBytes: 1000)
    }
    func stop() throws -> (durationSec: Double, micBytes: UInt64) {
        if let stopError { throw stopError }
        stopped = true
        return (durationSec: 42, micBytes: 2000)
    }
}

private final class MockSystem: SystemRecording {
    var startError: Error?
    var rotateError: Error?
    var stopError: Error?
    private(set) var started = false
    private(set) var pauseCalls: [Bool] = []

    func setPaused(_ paused: Bool) { pauseCalls.append(paused) }

    func start(systemURL: URL) async throws {
        if let startError { throw startError }
        started = true
    }
    func rotate(to url: URL) throws -> UInt64 {
        if let rotateError { throw rotateError }
        return 500
    }
    func stopRecording() async throws -> UInt64 {
        if let stopError { throw stopError }
        return 900
    }
}

private final class MockLevelTimer: LevelTimerControlling {
    private(set) var running = false
    func startLevelTimer() { running = true }
    func stopLevelTimer() { running = false }
}

private final class MockProbe: CallProbing {
    private(set) var running = false
    func startProbe(emit: @escaping ([String: Any]) -> Void) { running = true }
    func stopProbe() { running = false }
}

private final class MockPermissions: PermissionsProviding {
    private(set) var requestedTargets: [String] = []
    func currentPermissions() -> [String: Any] { ["event": "permissions", "microphone": "granted"] }
    func requestPermissions(target: String) async { requestedTargets.append(target) }
}

private struct Harness {
    let mic = MockMic()
    let system = MockSystem()
    let timer = MockLevelTimer()
    let probe = MockProbe()
    let permissions = MockPermissions()

    func router() -> CommandRouter {
        CommandRouter(
            mic: mic, system: system, levelTimer: timer, probe: probe,
            permissions: permissions, emitProbeEvent: { _ in })
    }
}

private func eventNames(_ events: [SidecarEvent]) -> [String] {
    events.compactMap { $0.jsonObject["event"] as? String }
}

@Suite("CommandRouter — старт и стоп")
struct RouterStartStopTests {
    @Test("успешный старт поднимает обе дорожки и таймер уровней")
    func startOk() async {
        let h = Harness()
        let events = await h.router().handle(.start(micPath: "/m.wav", systemPath: "/s.wav"))
        #expect(eventNames(events) == ["started"])
        #expect(h.mic.started)
        #expect(h.system.started)
        #expect(h.timer.running)
    }

    @Test("падение мика — фатальное, система не поднимается")
    func startMicFails() async {
        let h = Harness()
        h.mic.startError = Boom(what: "no device")
        let events = await h.router().handle(.start(micPath: "/m.wav", systemPath: "/s.wav"))
        #expect(eventNames(events) == ["error"])
        #expect(events[0].jsonObject["message"] as? String == "mic start failed: no device")
        #expect(!h.system.started)
        #expect(!h.timer.running)
    }

    @Test("падение системы откатывает уже стартовавший мик")
    func startSystemFailsRollsBackMic() async {
        let h = Harness()
        h.system.startError = Boom(what: "no tap")
        let events = await h.router().handle(.start(micPath: "/m.wav", systemPath: "/s.wav"))
        #expect(eventNames(events) == ["error"])
        // Мик не должен остаться писать в одиночку.
        #expect(h.mic.stopped)
        #expect(!h.timer.running)
    }

    @Test("stop гасит таймер и отдаёт байты обеих дорожек")
    func stopOk() async {
        let h = Harness()
        _ = await h.router().handle(.start(micPath: "/m.wav", systemPath: "/s.wav"))
        let events = await h.router().handle(.stop)
        #expect(eventNames(events) == ["stopped"])
        let json = events[0].jsonObject
        #expect(json["mic_bytes"] as? Int == 2000)
        #expect(json["system_bytes"] as? Int == 900)
        #expect(json["warning"] == nil)
        #expect(!h.timer.running)
    }

    @Test("падение системы на stop — не error: запись состоялась")
    func stopSystemFailsIsWarning() async {
        let h = Harness()
        h.system.stopError = Boom(what: "tap gone")
        let events = await h.router().handle(.stop)
        #expect(eventNames(events) == ["stopped"])
        let json = events[0].jsonObject
        #expect(json["system_bytes"] as? Int == 0)
        #expect(json["warning"] as? String == "system stop failed: tap gone")
    }
}

@Suite("CommandRouter — ротация (TD-06)")
struct RouterRotateTests {
    @Test("обе ноги ок — обычный rotated без warning")
    func rotateBothOk() async {
        let h = Harness()
        let events = await h.router().handle(.rotate(nextMicPath: "/1/m.wav", nextSystemPath: "/1/s.wav"))
        #expect(eventNames(events) == ["rotated"])
        let json = events[0].jsonObject
        #expect(json["mic_bytes"] as? Int == 1000)
        #expect(json["system_bytes"] as? Int == 500)
        #expect(json["warning"] == nil)
    }

    @Test("система упала после успешного мика — индекс всё равно продвигается")
    func rotateSystemFailsStillAdvances() async {
        // Ключевой сценарий TD-06: мик уже пишет в chunk k+1. Если не отдать
        // rotated, оркестратор не продвинет индекс и дорожки разъедутся.
        let h = Harness()
        h.system.rotateError = Boom(what: "ENOSPC")
        let events = await h.router().handle(.rotate(nextMicPath: "/1/m.wav", nextSystemPath: "/1/s.wav"))

        #expect(eventNames(events) == ["rotate_error", "rotated"])

        let err = events[0].jsonObject
        #expect(err["leg"] as? String == "system")
        #expect(err["mic_rotated"] as? Bool == true)

        let rotated = events[1].jsonObject
        #expect(rotated["mic_bytes"] as? Int == 1000)
        #expect(rotated["system_bytes"] as? Int == 0)
        #expect(rotated["warning"] as? String == "system rotate failed: ENOSPC")
        // Мик действительно переехал на новый файл.
        #expect(h.mic.rotatedTo == ["/1/m.wav"])
    }

    @Test("упал сам мик — ротации не было, индекс не двигаем")
    func rotateMicFailsDoesNotAdvance() async {
        let h = Harness()
        h.mic.rotateError = Boom(what: "closed")
        let events = await h.router().handle(.rotate(nextMicPath: "/1/m.wav", nextSystemPath: "/1/s.wav"))

        #expect(eventNames(events) == ["rotate_error"])
        let err = events[0].jsonObject
        #expect(err["leg"] as? String == "mic")
        #expect(err["mic_rotated"] as? Bool == false)
        // Никакого rotated — иначе оркестратор посчитал бы несуществующий chunk.
        #expect(!eventNames(events).contains("rotated"))
    }

    @Test("ни один сбой ротации не выглядит как фатальный error")
    func rotateNeverEmitsFatalError() async {
        for (micFails, sysFails) in [(true, false), (false, true), (true, true)] {
            let h = Harness()
            if micFails { h.mic.rotateError = Boom(what: "m") }
            if sysFails { h.system.rotateError = Boom(what: "s") }
            let events = await h.router().handle(
                .rotate(nextMicPath: "/1/m.wav", nextSystemPath: "/1/s.wav"))
            #expect(!eventNames(events).contains("error"),
                    "сбой ротации не должен убивать сессию (mic=\(micFails) sys=\(sysFails))")
        }
    }
}

@Suite("CommandRouter — прочее")
struct RouterMiscTests {
    @Test("ping отвечает pong")
    func ping() async {
        let events = await Harness().router().handle(.ping)
        #expect(eventNames(events) == ["pong"])
    }

    @Test("битая строка становится фатальным error")
    func badLine() async {
        let events = await Harness().router().handle(line: "не json")
        #expect(eventNames(events) == ["error"])
        #expect(events[0].jsonObject["message"] as? String == "invalid command line")
    }

    @Test("неизвестная команда — error с именем")
    func unknownLine() async {
        let events = await Harness().router().handle(line: #"{"cmd":"nope"}"#)
        #expect(events[0].jsonObject["message"] as? String == "unknown cmd: nope")
    }

    @Test("call_detect управляет пробником")
    func callDetect() async {
        let h = Harness()
        let r = h.router()
        #expect(eventNames(await r.handle(.callDetectStart)) == ["call_detect_started"])
        #expect(h.probe.running)
        #expect(eventNames(await r.handle(.callDetectStop)) == ["call_detect_stopped"])
        #expect(!h.probe.running)
    }

    @Test("request_permissions прокидывает target и отдаёт свежий статус")
    func permissions() async {
        let h = Harness()
        let events = await h.router().handle(.requestPermissions(target: "microphone"))
        #expect(h.permissions.requestedTargets == ["microphone"])
        #expect(events[0].jsonObject["event"] as? String == "permissions")
    }
}

@Suite("CommandRouter — пауза (TD-07)")
struct RouterPauseTests {
    @Test("pause останавливает захват на ОБЕИХ дорожках")
    func pauseStopsBothLegs() async {
        // Суть задачи: раньше пауза жила только в БД, сайдкар продолжал писать,
        // и сказанное «на паузе» уезжало в транскрипт и саммари.
        let h = Harness()
        let events = await h.router().handle(.pause)
        #expect(eventNames(events) == ["paused"])
        #expect(h.mic.pauseCalls == [true])
        #expect(h.system.pauseCalls == [true])
    }

    @Test("resume возвращает захват обеим дорожкам")
    func resumeRestoresBothLegs() async {
        let h = Harness()
        let r = h.router()
        _ = await r.handle(.pause)
        let events = await r.handle(.resume)
        #expect(eventNames(events) == ["resumed"])
        #expect(h.mic.pauseCalls == [true, false])
        #expect(h.system.pauseCalls == [true, false])
    }

    @Test("повторная пауза идемпотентна — не ошибка")
    func pauseIsIdempotent() async {
        // UI-кнопка и хоткей могут прийти почти одновременно.
        let h = Harness()
        let r = h.router()
        for _ in 0..<3 { #expect(eventNames(await r.handle(.pause)) == ["paused"]) }
        #expect(h.mic.pauseCalls == [true, true, true])
    }

    @Test("pause/resume разбираются из строки протокола")
    func parsedFromLine() async {
        let h = Harness()
        let r = h.router()
        #expect(eventNames(await r.handle(line: #"{"cmd":"pause"}"#)) == ["paused"])
        #expect(eventNames(await r.handle(line: #"{"cmd":"resume"}"#)) == ["resumed"])
        #expect(h.mic.pauseCalls == [true, false])
    }

    @Test("пауза не трогает таймер уровней — UI должен видеть, что приложение живо")
    func pauseKeepsLevelTimer() async {
        let h = Harness()
        let r = h.router()
        _ = await r.handle(.start(micPath: "/m.wav", systemPath: "/s.wav"))
        _ = await r.handle(.pause)
        #expect(h.timer.running)
    }
}
