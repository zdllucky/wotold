import Foundation
import Testing

@testable import WotoldAudioCore

/// [TD-21d] Уровень сигнала пишется из обработчика кадров и читается с
/// таймерной очереди. Раньше это был голый `var latestRms: Float` без всякой
/// синхронизации — гонка данных, которую strict concurrency Swift 6 уже не
/// пропускает.
@Suite("AtomicLevel (TD-21d)")
struct AtomicLevelTests {

    @Test("значение по умолчанию — ноль")
    func defaultsToZero() {
        #expect(AtomicLevel().value == 0)
    }

    @Test("set/reset читаются обратно")
    func setAndReset() {
        let level = AtomicLevel()
        level.set(0.42)
        #expect(abs(level.value - 0.42) < 1e-6)
        level.reset()
        #expect(level.value == 0)
    }

    @Test("значения зажимаются в 0…1")
    func clampsOutOfRange() {
        let level = AtomicLevel()
        level.set(1.7)
        #expect(level.value == 1)
        level.set(-0.3)
        #expect(level.value == 0)
    }

    @Test("NaN не залипает в индикаторе")
    func nanIsIgnored() {
        // RMS считается из float-буфера: одиночный NaN на входе раньше залип
        // бы навсегда — сравнения с NaN ложны, поэтому ни один последующий
        // clamp его бы не вытеснил.
        let level = AtomicLevel()
        level.set(0.5)
        level.set(.nan)
        #expect(abs(level.value - 0.5) < 1e-6, "NaN обязан быть отброшен")
        level.set(.infinity)
        #expect(abs(level.value - 0.5) < 1e-6, "бесконечность тоже")
    }

    @Test("одновременные чтения и записи не рвут значение")
    func concurrentAccessStaysInRange() async {
        // Не тест на гонку в строгом смысле (её ловит TSan, а не assert), но
        // фиксирует контракт: под параллельной нагрузкой наружу не вылезает
        // значение вне 0…1 и не падает.
        let level = AtomicLevel()
        await withTaskGroup(of: Void.self) { group in
            for i in 0..<8 {
                group.addTask {
                    for j in 0..<2_000 {
                        level.set(Float((i + j) % 100) / 100)
                    }
                }
            }
            for _ in 0..<8 {
                group.addTask {
                    for _ in 0..<2_000 {
                        let v = level.value
                        #expect(v >= 0 && v <= 1)
                    }
                }
            }
        }
    }
}
