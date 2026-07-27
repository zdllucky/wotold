import Foundation
import WotoldAudioCore

// JSON line protocol:
//   stdin  →  { "cmd": "start", "mic_path": "/abs/mic.wav", "system_path": "/abs/system.wav" }
//             { "cmd": "rotate", "next_mic_path": "/abs/...", "next_system_path": "/abs/..." }
//             { "cmd": "stop" }
//             { "cmd": "ping" }
//             { "cmd": "call_detect_start" }   [S2] standalone probe mode
//             { "cmd": "call_detect_stop"  }
//   stdout ←  { "event": "started" }
//             { "event": "level", "mic": 0.12, "system": 0.34 }  [B14] каждые 100ms
//             { "event": "rotated", "duration_sec": N, "mic_bytes": N, "system_bytes": N }
//                                   [+ "warning" если системная нога не ротировалась]
//             { "event": "rotate_error", "leg": "mic"|"system", "mic_rotated": bool,
//               "message": "..." }   [TD-06] НЕфатальное: запись продолжается
//             { "event": "stopped", "duration_sec": N, "mic_bytes": N, "system_bytes": N }
//             { "event": "error",   "message": "..." }   фатальное
//             { "event": "pong" }
//             { "event": "call_detect_started" }
//             { "event": "call_suggested", "bundle_id": "...", "app_name": "...", "reason": "..." }
//             { "event": "call_detect_stopped" }
//
// [TD-06] Разбор команд, решения и кодирование событий живут в WotoldAudioCore
// (тестируются моками). Здесь остаётся только проводка конкретных рекордеров.

@main
struct WotoldAudioMain {
    static let stdout = FileHandle.standardOutput

    static func emit(_ dict: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: dict, options: []) else {
            return
        }
        var line = data
        line.append(0x0a)
        stdout.write(line)
    }

    static func emitError(_ message: String) {
        emit(SidecarEvent.error(message).jsonObject)
    }

    static func main() async {
        let mic = AudioRecorder()
        guard #available(macOS 14.4, *) else {
            emitError("macOS 14.4+ required for system audio capture (Core Audio Process Tap)")
            return
        }
        let system = ProcessTapRecorder()

        // [TD-21e] Рекордер не знает про stdout — сообщает о потере и возврате
        // дорожки сюда. Событие идёт в общий поток вне очереди команд: оно
        // возникает само по себе, а не в ответ на команду.
        mic.onDeviceEvent = { event in emit(event.jsonObject) }
        // [TD-44] Системная дорожка сообщает о том же и туда же — до этого
        // фикса она молчала, и «система ушла в тишину» юзер узнавал только по
        // пустому транскрипту собеседника (правило 2, близнецы).
        system.onDeviceEvent = { event in emit(event.jsonObject) }

        let router = CommandRouter(
            mic: mic,
            system: system,
            levelTimer: LevelTimer(mic: mic, system: system, emit: emit),
            probe: CallActivityProbe(),
            permissions: SystemPermissions(),
            emitProbeEvent: emit
        )

        while let line = readLine(strippingNewline: true) {
            if line.isEmpty { continue }
            for event in await router.handle(line: line) {
                emit(event.jsonObject)
            }
        }
    }
}

// MARK: - Конформансы конкретных типов к протоколам ядра

extension AudioRecorder: MicRecording {}

@available(macOS 14.4, *)
extension ProcessTapRecorder: SystemRecording {
    /// Имя отличается от `stop()`, потому что конкретный метод возвращает
    /// `StopResult`, а протоколу нужен голый счётчик байт.
    func stopRecording() async throws -> UInt64 {
        try await stop().bytesWritten
    }
}

extension CallActivityProbe: CallProbing {
    func startProbe(emit: @escaping ([String: Any]) -> Void) { start(emit: emit) }
    func stopProbe() { stop() }
}

/// Разрешения macOS за протоколом — свободные функции из Permissions.swift.
final class SystemPermissions: PermissionsProviding {
    func currentPermissions() -> [String: Any] { permissionsEvent() }

    func requestPermissions(target: String) async {
        if target == "microphone" || target == "all" {
            _ = await requestMicrophoneAccess()
        }
        if target == "screen_recording" || target == "all" {
            _ = requestScreenRecordingAccess()
        }
        if target == "accessibility" || target == "all" {
            _ = requestAccessibilityAccess()
        }
    }
}

/// [B14] Таймер уровней: каждые 100 мс шлёт RMS обеих дорожек.
/// Держит конкретные рекордеры — поэтому живёт в executable, а не в ядре.
@available(macOS 14.4, *)
final class LevelTimer: LevelTimerControlling {
    private let queue = DispatchQueue(label: "app.wotold.macos-audio.level")
    private var timer: DispatchSourceTimer?
    private let mic: AudioRecorder
    private let system: ProcessTapRecorder
    private let emit: ([String: Any]) -> Void

    init(mic: AudioRecorder, system: ProcessTapRecorder, emit: @escaping ([String: Any]) -> Void) {
        self.mic = mic
        self.system = system
        self.emit = emit
    }

    /// Idempotent — повторный вызов переустанавливает таймер.
    func startLevelTimer() {
        stopLevelTimer()
        let t = DispatchSource.makeTimerSource(queue: queue)
        t.schedule(deadline: .now() + .milliseconds(100), repeating: .milliseconds(100))
        t.setEventHandler { [mic, system, emit] in
            // round до 4 знаков чтобы JSON был компактнее.
            let m = Double((mic.currentRms * 10_000).rounded() / 10_000)
            let s = Double((system.currentRms * 10_000).rounded() / 10_000)
            emit(SidecarEvent.level(mic: m, system: s).jsonObject)
        }
        t.resume()
        timer = t
    }

    func stopLevelTimer() {
        timer?.cancel()
        timer = nil
    }
}
