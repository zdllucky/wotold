// Обновления. Раньше `UpdateBanner` дёргал `invoke` напрямую и держал у себя
// копию Rust-структуры — при добавлении поля `urgency` копия молча разъехалась
// бы с бэкендом. Тип теперь один, здесь.
import { invoke } from '@tauri-apps/api/core';

/** Насколько обновление обязательно. Зеркало `updater::UpdateUrgency`. */
export type UpdateUrgency = 'optional' | 'mandatory';

/** Зеркало `updater::AvailableUpdate` (serde, snake_case). */
export interface AvailableUpdate {
  version: string;
  current_version: string;
  notes: string | null;
  pub_date: string | null;
  urgency: UpdateUrgency;
}

/**
 * Разовая проверка. Фоновая живёт в Rust (`spawn_updater_poll`) и приезжает
 * событием `updater:available` — эта функция для кнопки «Проверить
 * обновления», где пользователь ждёт ответа здесь и сейчас.
 */
export function checkForUpdate(): Promise<AvailableUpdate | null> {
  return invoke<AvailableUpdate | null>('check_for_update');
}

/**
 * Скачать, установить, перезапустить. При успехе не возвращается — процесс
 * завершается перезапуском.
 *
 * Гейта занятости здесь нет намеренно: его проходит только принудительное
 * обновление в Rust. Если пользователь нажал кнопку сам — момент выбрал он.
 */
export function applyUpdate(): Promise<void> {
  return invoke<void>('apply_update');
}

/** Событие фонового поллера. Payload — `AvailableUpdate`. */
export const UPDATER_AVAILABLE_EVENT = 'updater:available';
