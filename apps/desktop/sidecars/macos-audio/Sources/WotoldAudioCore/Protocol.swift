import Foundation

// [TD-06] Детерминированный слой NDJSON-протокола сайдкара: разбор команд и
// кодирование событий. Живёт в библиотеке, а не в executable, чтобы быть
// тестируемым без Core Audio и без разрешений macOS.

// MARK: - Команды

/// Команда, пришедшая на stdin.
public enum Command: Equatable {
    case ping
    case checkPermissions
    case requestPermissions(target: String)
    case start(micPath: String, systemPath: String)
    case rotate(nextMicPath: String, nextSystemPath: String)
    /// [TD-07] Приостановить/возобновить ЗАХВАТ, а не только счётчик времени.
    case pause
    case resume
    case stop
    case callDetectStart
    case callDetectStop
}

/// Почему строка не стала командой. Текст сообщений сохранён дословно —
/// Rust-сторона их только логирует, но менять формат без нужды не стоит.
public enum CommandParseError: Error, Equatable {
    case invalidLine
    case missingFields(String)
    case unknownCommand(String)

    public var message: String {
        switch self {
        case .invalidLine:
            return "invalid command line"
        case let .missingFields(m):
            return m
        case let .unknownCommand(cmd):
            return "unknown cmd: \(cmd)"
        }
    }
}

extension Command {
    /// Разобрать одну строку stdin. Пустые строки отсеивает вызывающий.
    public static func parse(line: String) -> Result<Command, CommandParseError> {
        guard
            let data = line.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let cmd = obj["cmd"] as? String
        else {
            return .failure(.invalidLine)
        }

        func nonEmpty(_ key: String) -> String? {
            guard let v = obj[key] as? String, !v.isEmpty else { return nil }
            return v
        }

        switch cmd {
        case "ping":
            return .success(.ping)
        case "check_permissions":
            return .success(.checkPermissions)
        case "request_permissions":
            return .success(.requestPermissions(target: (obj["target"] as? String) ?? "all"))
        case "start":
            guard let mic = nonEmpty("mic_path"), let system = nonEmpty("system_path") else {
                return .failure(.missingFields("mic_path and system_path required for start"))
            }
            return .success(.start(micPath: mic, systemPath: system))
        case "rotate":
            guard let mic = nonEmpty("next_mic_path"), let system = nonEmpty("next_system_path")
            else {
                return .failure(
                    .missingFields("next_mic_path and next_system_path required for rotate"))
            }
            return .success(.rotate(nextMicPath: mic, nextSystemPath: system))
        case "pause":
            return .success(.pause)
        case "resume":
            return .success(.resume)
        case "stop":
            return .success(.stop)
        case "call_detect_start":
            return .success(.callDetectStart)
        case "call_detect_stop":
            return .success(.callDetectStop)
        default:
            return .failure(.unknownCommand(cmd))
        }
    }
}

// MARK: - События

/// Какая дорожка не смогла ротироваться.
public enum RotateLeg: String, Equatable {
    case mic
    case system
}

/// Событие, уходящее в stdout.
///
/// [TD-06] Ключевое разделение: `error` — **фатальное** (запись остановлена
/// или не началась), `rotateError` — **операционное** (ротация не удалась, но
/// тап и IOProc живы и продолжают писать). Раньше оба шли как `error`, и
/// Rust-диспатчер убивал сессию из-за transient'а на ротации, теряя часовую
/// запись при целых WAV на диске.
public enum SidecarEvent {
    case pong
    case started
    case level(mic: Double, system: Double)
    case rotated(durationSec: Double, micBytes: UInt64, systemBytes: UInt64, warning: String?)
    case rotateError(leg: RotateLeg, micRotated: Bool, message: String)
    case stopped(durationSec: Double, micBytes: UInt64, systemBytes: UInt64, warning: String?)
    /// [TD-07] Захват фактически остановлен/возобновлён на обеих дорожках.
    case paused
    case resumed
    /// [TD-21e] Дорожка замолчала: кадры перестали приходить дольше порога.
    /// Операционное, как `rotateError` — запись продолжается, вторая дорожка
    /// жива, а эта может вернуться сама.
    case deviceLost(leg: RotateLeg, message: String)
    /// [TD-21e] Кадры вернулись. `gapSec` — сколько секунд дорожки потеряно.
    case deviceRecovered(leg: RotateLeg, gapSec: Double, restarted: Bool)
    case error(String)
    case callDetectStarted
    case callDetectStopped
    /// Готовый словарь (permissionsEvent, call_suggested) — собирается вне ядра.
    case raw([String: Any])

    public var jsonObject: [String: Any] {
        switch self {
        case .pong:
            return ["event": "pong"]
        case .started:
            return ["event": "started"]
        case let .level(mic, system):
            return ["event": "level", "mic": mic, "system": system]
        case let .rotated(durationSec, micBytes, systemBytes, warning):
            var d: [String: Any] = [
                "event": "rotated",
                "duration_sec": durationSec,
                "mic_bytes": Int(micBytes),
                "system_bytes": Int(systemBytes),
            ]
            if let warning { d["warning"] = warning }
            return d
        case let .rotateError(leg, micRotated, message):
            return [
                "event": "rotate_error",
                "leg": leg.rawValue,
                "mic_rotated": micRotated,
                "message": message,
            ]
        case let .stopped(durationSec, micBytes, systemBytes, warning):
            var d: [String: Any] = [
                "event": "stopped",
                "duration_sec": durationSec,
                "mic_bytes": Int(micBytes),
                "system_bytes": Int(systemBytes),
            ]
            if let warning { d["warning"] = warning }
            return d
        case .paused:
            return ["event": "paused"]
        case .resumed:
            return ["event": "resumed"]
        case let .deviceLost(leg, message):
            return ["event": "device_lost", "leg": leg.rawValue, "message": message]
        case let .deviceRecovered(leg, gapSec, restarted):
            return [
                "event": "device_recovered",
                "leg": leg.rawValue,
                "gap_sec": gapSec,
                "restarted": restarted,
            ]
        case let .error(message):
            return ["event": "error", "message": message]
        case .callDetectStarted:
            return ["event": "call_detect_started"]
        case .callDetectStopped:
            return ["event": "call_detect_stopped"]
        case let .raw(dict):
            return dict
        }
    }
}
