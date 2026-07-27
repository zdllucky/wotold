import Foundation
import os

/// [TD-21] Уровень сигнала, разделяемый между аудио-колбэком и таймером.
///
/// Раньше оба рекордера держали `private var latestRms: Float`, писали его из
/// обработчика кадров (serial queue рекордера) и читали из `LevelTimer`
/// (другая очередь) вообще без синхронизации, с комментарием
/// «не thread-safe чтение — но atomic-fast enough». Для `Float` это не так:
/// одновременные чтение и запись без синхронизации — гонка данных по модели
/// памяти, и под strict concurrency Swift 6 это уже не мнение компилятора, а
/// диагностика.
///
/// `OSAllocatedUnfairLock` выбран сознательно: доступен с macOS 13 (цель
/// пакета — macOS 14), не аллоцирует на каждое обращение и не блокирует
/// надолго — критично, потому что писатель здесь сидит на аудио-пути.
/// `Synchronization.Mutex` подошёл бы лучше по эргономике, но требует
/// macOS 15 и поднял бы минимальную версию всего сайдкара.
public final class AtomicLevel: @unchecked Sendable {
    private let storage: OSAllocatedUnfairLock<Float>

    public init(_ initial: Float = 0) {
        storage = OSAllocatedUnfairLock(initialState: initial)
    }

    /// Текущее значение. Зовётся с таймерной очереди.
    public var value: Float {
        storage.withLock { $0 }
    }

    /// Записать значение. Зовётся из обработчика кадров.
    ///
    /// Нечисловые значения отбрасываются: RMS считается из float-буфера, и
    /// один NaN, попав сюда, залипал бы в индикаторе до конца записи —
    /// сравнения с NaN всегда ложны, так что его не вытеснил бы никакой
    /// последующий clamp на стороне читателя.
    public func set(_ newValue: Float) {
        guard newValue.isFinite else { return }
        storage.withLock { $0 = max(0, min(1, newValue)) }
    }

    /// Сбросить в ноль — на паузе и на остановке.
    public func reset() {
        storage.withLock { $0 = 0 }
    }
}
