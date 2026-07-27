import Foundation
import Testing

@testable import WotoldAudioCore

// [TD-06] Первые тесты сайдкара. До этого их было ноль: всё жило в одном
// executableTarget, импортировать было нечего.

// MARK: - Разбор команд

@Suite("Command.parse")
struct CommandParseTests {
    @Test("валидные команды разбираются")
    func validCommands() {
        #expect(Command.parse(line: #"{"cmd":"ping"}"#) == .success(.ping))
        #expect(Command.parse(line: #"{"cmd":"stop"}"#) == .success(.stop))
        #expect(Command.parse(line: #"{"cmd":"check_permissions"}"#) == .success(.checkPermissions))
        #expect(
            Command.parse(line: #"{"cmd":"call_detect_start"}"#) == .success(.callDetectStart))
        #expect(Command.parse(line: #"{"cmd":"call_detect_stop"}"#) == .success(.callDetectStop))
    }

    @Test("start несёт оба пути")
    func startPaths() {
        let line = #"{"cmd":"start","mic_path":"/a/mic.wav","system_path":"/a/sys.wav"}"#
        #expect(
            Command.parse(line: line) == .success(.start(micPath: "/a/mic.wav", systemPath: "/a/sys.wav")))
    }

    @Test("rotate несёт оба пути")
    func rotatePaths() {
        let line = #"{"cmd":"rotate","next_mic_path":"/a/1/mic.wav","next_system_path":"/a/1/sys.wav"}"#
        #expect(
            Command.parse(line: line)
                == .success(.rotate(nextMicPath: "/a/1/mic.wav", nextSystemPath: "/a/1/sys.wav")))
    }

    @Test("request_permissions по умолчанию all")
    func requestPermissionsDefault() {
        #expect(
            Command.parse(line: #"{"cmd":"request_permissions"}"#)
                == .success(.requestPermissions(target: "all")))
        #expect(
            Command.parse(line: #"{"cmd":"request_permissions","target":"microphone"}"#)
                == .success(.requestPermissions(target: "microphone")))
    }

    @Test("битый ввод не проходит")
    func invalidInput() {
        #expect(Command.parse(line: "не json") == .failure(.invalidLine))
        #expect(Command.parse(line: "{}") == .failure(.invalidLine))
        #expect(Command.parse(line: #"{"event":"pong"}"#) == .failure(.invalidLine))
    }

    @Test("неизвестная команда называет себя")
    func unknownCommand() {
        #expect(Command.parse(line: #"{"cmd":"launch_missiles"}"#)
            == .failure(.unknownCommand("launch_missiles")))
    }

    @Test("обязательные поля проверяются, пустая строка не считается путём")
    func missingFields() {
        for line in [
            #"{"cmd":"start"}"#,
            #"{"cmd":"start","mic_path":"/a/mic.wav"}"#,
            #"{"cmd":"start","mic_path":"","system_path":"/a/s.wav"}"#,
        ] {
            #expect(Command.parse(line: line)
                == .failure(.missingFields("mic_path and system_path required for start")))
        }
        #expect(Command.parse(line: #"{"cmd":"rotate","next_mic_path":"/a"}"#)
            == .failure(.missingFields("next_mic_path and next_system_path required for rotate")))
    }
}

// MARK: - Кодирование событий

@Suite("SidecarEvent.jsonObject")
struct SidecarEventTests {
    @Test("rotate_error — отдельное событие с ногой и флагом")
    func rotateErrorShape() {
        let json = SidecarEvent
            .rotateError(leg: .system, micRotated: true, message: "disk full")
            .jsonObject
        #expect(json["event"] as? String == "rotate_error")
        #expect(json["leg"] as? String == "system")
        #expect(json["mic_rotated"] as? Bool == true)
        #expect(json["message"] as? String == "disk full")
    }

    @Test("rotate_error отличим от фатального error")
    func rotateErrorIsNotError() {
        // Ровно это различие Rust-диспатчер использует, чтобы не убивать сессию.
        let fatal = SidecarEvent.error("start failed").jsonObject["event"] as? String
        let nonFatal = SidecarEvent
            .rotateError(leg: .mic, micRotated: false, message: "x").jsonObject["event"] as? String
        #expect(fatal == "error")
        #expect(nonFatal == "rotate_error")
        #expect(fatal != nonFatal)
    }

    @Test("warning появляется только когда он есть")
    func warningIsOptional() {
        let clean = SidecarEvent
            .rotated(durationSec: 1, micBytes: 2, systemBytes: 3, warning: nil).jsonObject
        #expect(clean["warning"] == nil)
        #expect(clean["system_bytes"] as? Int == 3)

        let degraded = SidecarEvent
            .rotated(durationSec: 1, micBytes: 2, systemBytes: 0, warning: "нет системы").jsonObject
        #expect(degraded["warning"] as? String == "нет системы")
    }

    @Test("события сериализуются в JSON без потерь")
    func serialisable() throws {
        let events: [SidecarEvent] = [
            .pong, .started, .level(mic: 0.1, system: 0.2),
            .rotated(durationSec: 1.5, micBytes: 10, systemBytes: 20, warning: nil),
            .rotateError(leg: .mic, micRotated: false, message: "m"),
            .stopped(durationSec: 2, micBytes: 1, systemBytes: 0, warning: "w"),
            .error("boom"), .callDetectStarted, .callDetectStopped,
        ]
        for e in events {
            #expect(JSONSerialization.isValidJSONObject(e.jsonObject))
            _ = try JSONSerialization.data(withJSONObject: e.jsonObject)
        }
    }
}
