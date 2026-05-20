import Foundation

// JSON line protocol:
//   stdin  →  { "cmd": "start", "mic_path": "/abs/mic.wav", "system_path": "/abs/system.wav" }
//             { "cmd": "stop" }
//             { "cmd": "ping" }
//   stdout ←  { "event": "started" }
//             { "event": "stopped", "duration_sec": N, "mic_bytes": N, "system_bytes": N }
//             { "event": "error",   "message": "..." }
//             { "event": "pong" }

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

    static func main() async {
        let mic = AudioRecorder()
        let system = SystemAudioRecorder()

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

            case "stop":
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

            default:
                emitError("unknown cmd: \(cmd)")
            }
        }
    }
}
