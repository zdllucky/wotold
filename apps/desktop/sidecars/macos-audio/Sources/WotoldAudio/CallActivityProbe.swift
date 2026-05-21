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
    private let pollInterval: DispatchTimeInterval = .milliseconds(1500)

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
        let micBusy = currentInputDeviceBusy()
        let frontmost = currentFrontmostWhitelisted()

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

    /// kAudioDevicePropertyDeviceIsRunningSomewhere на default input device.
    /// 1 ⇒ кто-то (мы или сторонний процесс) активно читает с этого устройства.
    /// Не различает "мы" vs "сторонний" — Rust-сторона учитывает что наша
    /// собственная запись (recording session) тоже сделает mic busy, поэтому
    /// probe должен быть выключен пока recording == active.
    private func currentInputDeviceBusy() -> Bool {
        var deviceID = AudioDeviceID(0)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultInputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let status = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject),
            &address,
            0, nil,
            &size,
            &deviceID
        )
        guard status == noErr, deviceID != 0 else { return false }

        var running = UInt32(0)
        var runningSize = UInt32(MemoryLayout<UInt32>.size)
        var runningAddress = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyDeviceIsRunningSomewhere,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let runStatus = AudioObjectGetPropertyData(
            deviceID,
            &runningAddress,
            0, nil,
            &runningSize,
            &running
        )
        return runStatus == noErr && running == 1
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
