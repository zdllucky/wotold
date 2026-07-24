import Foundation

// [TD-06] Роутер команд: единственное место, где решается, что делать с
// командой и какие события отдать. Зависит только от протоколов, поэтому
// тестируется моками — без Core Audio, микрофона и macOS-разрешений.

/// Микрофонная дорожка (AVAudioEngine).
public protocol MicRecording: AnyObject {
    func start(micURL: URL) throws
    /// [TD-07] На паузе кадры дропаются до записи в WAV.
    func setPaused(_ paused: Bool)
    func rotate(to url: URL) throws -> (durationSec: Double, micBytes: UInt64)
    func stop() throws -> (durationSec: Double, micBytes: UInt64)
}

/// Системная дорожка (Core Audio process tap).
///
/// `stopRecording` вместо `stop` намеренно: у конкретного `ProcessTapRecorder`
/// метод `stop()` возвращает `StopResult`, и одноимённое требование протокола
/// с другим типом не даёт автоматического соответствия.
public protocol SystemRecording: AnyObject {
    func start(systemURL: URL) async throws
    /// [TD-07] См. `MicRecording.setPaused`.
    func setPaused(_ paused: Bool)
    func rotate(to url: URL) throws -> UInt64
    func stopRecording() async throws -> UInt64
}

/// Таймер уровней (100 мс). Живёт вне ядра — ему нужны конкретные рекордеры.
public protocol LevelTimerControlling: AnyObject {
    func startLevelTimer()
    func stopLevelTimer()
}

/// Пробник «идёт ли звонок» (S2).
public protocol CallProbing: AnyObject {
    func startProbe(emit: @escaping ([String: Any]) -> Void)
    func stopProbe()
}

/// Разрешения macOS.
public protocol PermissionsProviding: AnyObject {
    func currentPermissions() -> [String: Any]
    func requestPermissions(target: String) async
}

public final class CommandRouter {
    private let mic: any MicRecording
    private let system: any SystemRecording
    private let levelTimer: any LevelTimerControlling
    private let probe: any CallProbing
    private let permissions: any PermissionsProviding
    private let emitProbeEvent: ([String: Any]) -> Void

    public init(
        mic: any MicRecording,
        system: any SystemRecording,
        levelTimer: any LevelTimerControlling,
        probe: any CallProbing,
        permissions: any PermissionsProviding,
        emitProbeEvent: @escaping ([String: Any]) -> Void
    ) {
        self.mic = mic
        self.system = system
        self.levelTimer = levelTimer
        self.probe = probe
        self.permissions = permissions
        self.emitProbeEvent = emitProbeEvent
    }

    /// Разобрать строку и выполнить. Ошибка разбора превращается в фатальное
    /// `error` — как и было до TD-06.
    public func handle(line: String) async -> [SidecarEvent] {
        switch Command.parse(line: line) {
        case let .success(command):
            return await handle(command)
        case let .failure(err):
            return [.error(err.message)]
        }
    }

    public func handle(_ command: Command) async -> [SidecarEvent] {
        switch command {
        case .ping:
            return [.pong]

        case .checkPermissions:
            return [.raw(permissions.currentPermissions())]

        case let .requestPermissions(target):
            await permissions.requestPermissions(target: target)
            return [.raw(permissions.currentPermissions())]

        case let .start(micPath, systemPath):
            do {
                try mic.start(micURL: URL(fileURLWithPath: micPath))
            } catch {
                return [.error("mic start failed: \(error.localizedDescription)")]
            }
            do {
                try await system.start(systemURL: URL(fileURLWithPath: systemPath))
            } catch {
                // Откатываем мик, иначе останется писать в одиночку.
                _ = try? mic.stop()
                return [.error("system start failed: \(error.localizedDescription)")]
            }
            levelTimer.startLevelTimer()
            return [.started]

        case let .rotate(nextMic, nextSystem):
            return rotate(nextMic: nextMic, nextSystem: nextSystem)

        // [TD-07] Пауза опускается до уровня захвата. Раньше она жила только в
        // БД: сайдкар о ней не знал и продолжал писать кадры, поэтому сказанное
        // «на паузе» попадало в WAV, в транскрипт и в саммари. Идемпотентно —
        // повторная пауза не ошибка (UI и хоткей могут прийти одновременно).
        case .pause:
            mic.setPaused(true)
            system.setPaused(true)
            return [.paused]

        case .resume:
            mic.setPaused(false)
            system.setPaused(false)
            return [.resumed]

        case .stop:
            levelTimer.stopLevelTimer()
            let micResult: (durationSec: Double, micBytes: UInt64)
            do {
                micResult = try mic.stop()
            } catch {
                return [.error("mic stop failed: \(error.localizedDescription)")]
            }
            do {
                let sysBytes = try await system.stopRecording()
                return [
                    .stopped(
                        durationSec: micResult.durationSec, micBytes: micResult.micBytes,
                        systemBytes: sysBytes, warning: nil)
                ]
            } catch {
                // Мик уже остановлен и закрыт корректно — отдаём stopped с
                // предупреждением, а не error: запись состоялась.
                return [
                    .stopped(
                        durationSec: micResult.durationSec, micBytes: micResult.micBytes,
                        systemBytes: 0,
                        warning: "system stop failed: \(error.localizedDescription)")
                ]
            }

        case .callDetectStart:
            probe.startProbe(emit: emitProbeEvent)
            return [.callDetectStarted]

        case .callDetectStop:
            probe.stopProbe()
            return [.callDetectStopped]
        }
    }

    /// Ротация chunk'а.
    ///
    /// [TD-06] Асимметрия, ради которой всё затевалось: `mic.rotate` может
    /// пройти, а `system.rotate` — упасть. Мик в этот момент уже пишет в chunk
    /// k+1, поэтому индекс обязан продвинуться, иначе дорожки разъедутся на
    /// целый chunk. Отдаём `rotated` (с нулевыми system-байтами и warning'ом,
    /// по образцу stop-ветки) И `rotate_error` — первое держит индексы
    /// выровненными, второе сообщает Rust-стороне о деградации.
    ///
    /// Если упал сам мик — ротации не было, индекс не двигаем.
    private func rotate(nextMic: String, nextSystem: String) -> [SidecarEvent] {
        let micResult: (durationSec: Double, micBytes: UInt64)
        do {
            micResult = try mic.rotate(to: URL(fileURLWithPath: nextMic))
        } catch {
            return [
                .rotateError(
                    leg: .mic, micRotated: false,
                    message: "mic rotate failed: \(error.localizedDescription)")
            ]
        }

        do {
            let sysBytes = try system.rotate(to: URL(fileURLWithPath: nextSystem))
            return [
                .rotated(
                    durationSec: micResult.durationSec, micBytes: micResult.micBytes,
                    systemBytes: sysBytes, warning: nil)
            ]
        } catch {
            let message = "system rotate failed: \(error.localizedDescription)"
            return [
                .rotateError(leg: .system, micRotated: true, message: message),
                .rotated(
                    durationSec: micResult.durationSec, micBytes: micResult.micBytes,
                    systemBytes: 0, warning: message),
            ]
        }
    }
}
