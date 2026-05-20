import AVFoundation
import CoreGraphics

// Permission helpers для protocol-команд check_permissions / request_permissions.
// См. App.swift и M1.3 паспорта.

enum PermissionStatus: String {
    case granted = "granted"
    case denied = "denied"
    case notDetermined = "not_determined"
    case restricted = "restricted"
    case unknown = "unknown"
}

func currentMicrophoneStatus() -> PermissionStatus {
    let status = AVCaptureDevice.authorizationStatus(for: .audio)
    switch status {
    case .notDetermined: return .notDetermined
    case .restricted: return .restricted
    case .denied: return .denied
    case .authorized: return .granted
    @unknown default: return .unknown
    }
}

func currentScreenRecordingStatus() -> PermissionStatus {
    // CGPreflightScreenCaptureAccess не различает not_determined и denied —
    // оба возвращают false. Без приватных API (TCC.db) различить нельзя.
    // На стороне приложения отделим первый запрос флагом «уже спрашивали».
    if CGPreflightScreenCaptureAccess() {
        return .granted
    }
    return .denied
}

/// Просит микрофон. Блокирует до выбора пользователя (или сразу возвращает
/// если уже granted/denied).
func requestMicrophoneAccess() async -> Bool {
    return await AVCaptureDevice.requestAccess(for: .audio)
}

/// Фаирит системный диалог Screen Recording. Возвращает текущий статус
/// (не дожидается выбора). Полный апдейт прав вступает в силу при следующем
/// процессе — это особенность macOS TCC.
func requestScreenRecordingAccess() -> Bool {
    return CGRequestScreenCaptureAccess()
}

func permissionsEvent() -> [String: Any] {
    return [
        "event": "permissions",
        "microphone": currentMicrophoneStatus().rawValue,
        "screen_recording": currentScreenRecordingStatus().rawValue,
    ]
}
