import AppKit
import AVFoundation
import CoreAudio
import Foundation

// [S2] Call-activity probe. Опрашивает Core Audio default input device:
//   kAudioDevicePropertyDeviceIsRunningSomewhere == 1  ⇒  микрофон занят другим
//   процессом (Zoom/Teams/Meet/FaceTime/...). Параллельно слушает NSWorkspace
//   frontmost app — выбрасываем `call_suggested` только если активное приложение
//   из whitelist'а. Никакая audio-дорожка чужого приложения не читается — мы
//   видим лишь "busy" флаг, владельца не знаем.
//
// R3 deviation passport: opt-in, default OFF (settings CALL_DETECT_ENABLED).
//
// Reset logic: state == .suggested сбрасывается, когда mic перестаёт быть busy
// ИЛИ frontmost app выходит из whitelist'а. Cooldown per-app живёт в Rust
// (in-memory HashMap, рестарт обнуляет).

final class CallActivityProbe {
    enum State {
        case idle              // микрофон свободен
        case detected          // микрофон занят, но без whitelist'а / ждём вторую проверку
        case suggested(String) // событие уже выброшено, ждём reset
    }

    /// Bundle IDs приложений-звонилок. Браузеры включены чтобы поймать Google
    /// Meet, который живёт во вкладке (не различим от других вкладок —
    /// false-positive риск accepted, юзер увидит подсказку только когда
    /// действительно начал звонок и mic занят).
    static let bundleWhitelist: [String: String] = [
        "us.zoom.xos": "Zoom",
        "us.zoom.us": "Zoom",
        "com.microsoft.teams2": "Microsoft Teams",
        "com.microsoft.teams": "Microsoft Teams",
        "com.apple.FaceTime": "FaceTime",
        "com.hnc.Discord": "Discord",
        "com.tdesktop.Telegram": "Telegram",
        "ru.keepcoder.Telegram": "Telegram",
        "com.skype.skype": "Skype",
        "com.microsoft.SkypeForBusiness": "Skype",
        "com.cisco.webexmeetingsapp": "Webex",
        "com.google.Chrome": "Google Chrome",
        "com.apple.Safari": "Safari",
        "company.thebrowser.Browser": "Arc",
        "org.mozilla.firefox": "Firefox",
        "com.microsoft.edgemac": "Microsoft Edge",
    ]

    private let queue = DispatchQueue(label: "app.wotold.macos-audio.call-probe")
    private var timer: DispatchSourceTimer?
    private var state: State = .idle
    private let pollInterval: DispatchTimeInterval = .milliseconds(1000)
    /// [S8 diag] Last logged state — чтобы не спамить stderr одной и той же тривией
    /// (мы хотим видеть transitions, не каждый tick).
    private var lastLoggedSig: String = ""

    func start(emit: @escaping (_ dict: [String: Any]) -> Void) {
        stop()
        let t = DispatchSource.makeTimerSource(queue: queue)
        t.schedule(deadline: .now() + pollInterval, repeating: pollInterval)
        t.setEventHandler { [weak self] in
            self?.tick(emit: emit)
        }
        t.resume()
        timer = t
    }

    func stop() {
        timer?.cancel()
        timer = nil
        state = .idle
    }

    private func tick(emit: @escaping (_ dict: [String: Any]) -> Void) {
        let micBusy = anyInputDeviceBusy()
        let frontmost = currentFrontmostWhitelisted()
        let frontmostBundleAny = NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? "?"

        // [S8 diag] Edge log: emit stderr only on signature change, чтобы не
        // спамить логи. Rust forwarder проводит stderr в tauri-plugin-log
        // (см. call_detect dispatcher).
        let stateStr: String
        switch state {
        case .idle: stateStr = "idle"
        case .detected: stateStr = "detected"
        case .suggested(let b): stateStr = "suggested(\(b))"
        }
        let sig = "\(stateStr)|mic=\(micBusy)|front=\(frontmostBundleAny)"
        if sig != lastLoggedSig {
            lastLoggedSig = sig
            FileHandle.standardError.write(
                ("call-probe tick: " + sig + "\n").data(using: .utf8) ?? Data()
            )
        }

        switch state {
        case .idle:
            if micBusy, let (bundleId, name) = frontmost {
                state = .suggested(bundleId)
                emit([
                    "event": "call_suggested",
                    "bundle_id": bundleId,
                    "app_name": name,
                    "reason": "mic_busy_whitelisted_frontmost",
                ])
            } else if micBusy {
                state = .detected
            }

        case .detected:
            if !micBusy {
                state = .idle
            } else if let (bundleId, name) = frontmost {
                state = .suggested(bundleId)
                emit([
                    "event": "call_suggested",
                    "bundle_id": bundleId,
                    "app_name": name,
                    "reason": "mic_busy_whitelisted_frontmost",
                ])
            }

        case .suggested(let lockedBundle):
            if !micBusy {
                state = .idle
            } else if let (bundleId, _) = frontmost, bundleId != lockedBundle {
                // Юзер сменил приложение — пускаем нового кандидата следующим
                // tick'ом, чтобы Rust-cooldown увидел отдельное событие.
                state = .detected
            }
        }
    }

    /// [S8] Check ALL audio input devices — не только default. Teams/Zoom/etc
    /// могут переключаться на USB headset / virtual device, и default input
    /// при этом останется "MacBook Pro Microphone" idle. Iterate `kAudioHardware
    /// PropertyDevices`, фильтр по input streams (HasProperty на input scope),
    /// возвращаем true как только нашли busy.
    private func anyInputDeviceBusy() -> Bool {
        var addr = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDevices,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var dataSize: UInt32 = 0
        var status = AudioObjectGetPropertyDataSize(
            AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &dataSize
        )
        guard status == noErr else { return false }
        let count = Int(dataSize) / MemoryLayout<AudioDeviceID>.size
        guard count > 0 else { return false }

        var devices = [AudioDeviceID](repeating: 0, count: count)
        status = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &dataSize, &devices
        )
        guard status == noErr else { return false }

        for dev in devices {
            // Skip output-only devices: ask for input streams via
            // `kAudioDevicePropertyStreamConfiguration` на input scope; если
            // 0 streams — output-only (наушники, динамики, виртуальный output).
            var streamCfgAddr = AudioObjectPropertyAddress(
                mSelector: kAudioDevicePropertyStreamConfiguration,
                mScope: kAudioObjectPropertyScopeInput,
                mElement: kAudioObjectPropertyElementMain
            )
            var streamSize: UInt32 = 0
            status = AudioObjectGetPropertyDataSize(
                dev, &streamCfgAddr, 0, nil, &streamSize
            )
            if status != noErr || streamSize == 0 { continue }
            let bufList = UnsafeMutablePointer<AudioBufferList>.allocate(
                capacity: Int(streamSize)
            )
            defer { bufList.deallocate() }
            status = AudioObjectGetPropertyData(
                dev, &streamCfgAddr, 0, nil, &streamSize, bufList
            )
            if status != noErr { continue }
            let buffers = UnsafeMutableAudioBufferListPointer(bufList)
            let channels = buffers.reduce(0) { $0 + Int($1.mNumberChannels) }
            if channels == 0 { continue }

            var running = UInt32(0)
            var runningSize = UInt32(MemoryLayout<UInt32>.size)
            var runningAddr = AudioObjectPropertyAddress(
                mSelector: kAudioDevicePropertyDeviceIsRunningSomewhere,
                mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMain
            )
            status = AudioObjectGetPropertyData(
                dev, &runningAddr, 0, nil, &runningSize, &running
            )
            if status == noErr, running == 1 {
                return true
            }
        }
        return false
    }

    /// (bundle_id, displayName) если frontmost app в whitelist'е, иначе nil.
    private func currentFrontmostWhitelisted() -> (String, String)? {
        guard let app = NSWorkspace.shared.frontmostApplication,
              let bundleId = app.bundleIdentifier
        else { return nil }
        guard let display = Self.bundleWhitelist[bundleId] else { return nil }
        return (bundleId, display)
    }
}
