import Foundation

// JSON line protocol:
//   stdin  →  { "cmd": "start", "mic_path": "/abs/mic.wav", "system_path": "/abs/system.wav" }
//             { "cmd": "stop" }
//             { "cmd": "ping" }
//             { "cmd": "call_detect_start" }   [S2] standalone probe mode
//             { "cmd": "call_detect_stop"  }
//   stdout ←  { "event": "started" }
//             { "event": "level", "mic": 0.12, "system": 0.34 }  [B14] каждые 100ms
//             { "event": "stopped", "duration_sec": N, "mic_bytes": N, "system_bytes": N }
//             { "event": "error",   "message": "..." }
//             { "event": "pong" }
//             { "event": "call_detect_started" }
//             { "event": "call_suggested", "bundle_id": "...", "app_name": "...", "reason": "..." }
//             { "event": "call_detect_stopped" }

@main
struct WotoldAudioMain {
    static let stdout = FileHandle.standardOutput

    static func emit(_ dict: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: dict, options: []) else { return }
        var line = data
        line.append(0x0a)
        stdout.write(line)
    }

    static func emitError(_ message: String) {
        emit(["event": "error", "message": message])
    }

    static let levelQueue = DispatchQueue(label: "app.wotold.macos-audio.level")
    static var levelTimer: DispatchSourceTimer?

    // [S2] Долгоживущий probe — гоняется параллельно с (или вместо) recording.
    // Управляется командами call_detect_start / call_detect_stop. Никаких
    // shared resources с AudioRecorder/SystemAudioRecorder: только Core Audio
    // флаг + NSWorkspace.frontmostApplication, обе read-only sources.
    static let callProbe = CallActivityProbe()

    // [B14] Start a 100ms repeating timer that emits {"event":"level"} с
    // current RMS из mic + system recorders. Idempotent — повторный вызов
    // переустанавливает таймер.
    @available(macOS 14.4, *)
    static func startLevelTimer(mic: AudioRecorder, system: ProcessTapRecorder) {
        stopLevelTimer()
        let t = DispatchSource.makeTimerSource(queue: levelQueue)
        t.schedule(deadline: .now() + .milliseconds(100), repeating: .milliseconds(100))
        t.setEventHandler {
            // round to 4 знач знака чтобы JSON был компактнее.
            let m = (mic.currentRms * 10_000).rounded() / 10_000
            let s = (system.currentRms * 10_000).rounded() / 10_000
            emit(["event": "level", "mic": m, "system": s])
        }
        t.resume()
        levelTimer = t
    }

    static func stopLevelTimer() {
        levelTimer?.cancel()
        levelTimer = nil
    }

    static func main() async {
        let mic = AudioRecorder()
        guard #available(macOS 14.4, *) else {
            emitError("macOS 14.4+ required for system audio capture (Core Audio Process Tap)")
            return
        }
        let system = ProcessTapRecorder()

        while let line = readLine(strippingNewline: true) {
            if line.isEmpty { continue }

            guard
                let data = line.data(using: .utf8),
                let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                let cmd = obj["cmd"] as? String
            else {
                emitError("invalid command line")
                continue
            }

            switch cmd {
            case "ping":
                emit(["event": "pong"])

            case "check_permissions":
                emit(permissionsEvent())

            case "request_permissions":
                let target = (obj["target"] as? String) ?? "all"
                if target == "microphone" || target == "all" {
                    _ = await requestMicrophoneAccess()
                }
                if target == "screen_recording" || target == "all" {
                    _ = requestScreenRecordingAccess()
                }
                if target == "accessibility" || target == "all" {
                    _ = requestAccessibilityAccess()
                }
                emit(permissionsEvent())

            case "start":
                guard let micPath = obj["mic_path"] as? String,
                      let systemPath = obj["system_path"] as? String,
                      !micPath.isEmpty, !systemPath.isEmpty
                else {
                    emitError("mic_path and system_path required for start")
                    continue
                }

                do {
                    try mic.start(micURL: URL(fileURLWithPath: micPath))
                } catch {
                    emitError("mic start failed: \(error.localizedDescription)")
                    continue
                }
                do {
                    try await system.start(systemURL: URL(fileURLWithPath: systemPath))
                } catch {
                    _ = try? mic.stop()
                    emitError("system start failed: \(error.localizedDescription)")
                    continue
                }
                emit(["event": "started"])
                startLevelTimer(mic: mic, system: system)

            case "stop":
                stopLevelTimer()
                let micResult: (durationSec: Double, micBytes: UInt64)
                do {
                    micResult = try mic.stop()
                } catch {
                    emitError("mic stop failed: \(error.localizedDescription)")
                    continue
                }

                let sysBytes: UInt64
                do {
                    sysBytes = try await system.stop().bytesWritten
                } catch {
                    // Mic уже остановлен и закрыт корректно. Сообщаем что system
                    // частично провалился; вызывающая сторона должна это видеть.
                    emit([
                        "event": "stopped",
                        "duration_sec": micResult.durationSec,
                        "mic_bytes": Int(micResult.micBytes),
                        "system_bytes": 0,
                        "warning": "system stop failed: \(error.localizedDescription)",
                    ])
                    continue
                }

                emit([
                    "event": "stopped",
                    "duration_sec": micResult.durationSec,
                    "mic_bytes": Int(micResult.micBytes),
                    "system_bytes": Int(sysBytes),
                ])

            case "call_detect_start":
                callProbe.start { event in
                    emit(event)
                }
                emit(["event": "call_detect_started"])

            case "call_detect_stop":
                callProbe.stop()
                emit(["event": "call_detect_stopped"])

            default:
                emitError("unknown cmd: \(cmd)")
            }
        }
    }
}
