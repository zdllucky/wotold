import Foundation

// JSON line protocol:
//   stdin  →  { "cmd": "start", "mic_path": "/abs/path/mic.wav" }
//             { "cmd": "stop" }
//             { "cmd": "ping" }
//   stdout ←  { "event": "started" }
//             { "event": "stopped", "duration_sec": 12.3, "mic_bytes": 393216 }
//             { "event": "error",   "message": "..." }
//             { "event": "pong" }

let stdout = FileHandle.standardOutput
let stderr = FileHandle.standardError

func emit(_ dict: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: dict, options: []) else { return }
    var line = data
    line.append(0x0a)  // newline
    stdout.write(line)
}

func emitError(_ message: String) {
    emit(["event": "error", "message": message])
}

let recorder = AudioRecorder()

while let line = readLine(strippingNewline: true) {
    if line.isEmpty { continue }
    guard let data = line.data(using: .utf8),
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
        guard let micPath = obj["mic_path"] as? String, !micPath.isEmpty else {
            emitError("mic_path required for start")
            continue
        }
        do {
            try recorder.start(micURL: URL(fileURLWithPath: micPath))
            emit(["event": "started"])
        } catch {
            emitError("start failed: \(error.localizedDescription)")
        }

    case "stop":
        do {
            let result = try recorder.stop()
            emit([
                "event": "stopped",
                "duration_sec": result.durationSec,
                "mic_bytes": Int(result.micBytes),
            ])
        } catch {
            emitError("stop failed: \(error.localizedDescription)")
        }

    default:
        emitError("unknown cmd: \(cmd)")
    }
}
