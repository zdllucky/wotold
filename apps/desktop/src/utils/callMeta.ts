// [B17] Pure helpers extracted from CallDetailPage для unit testability.
// Header meta formatting, duration humanization, fallback title, speaker
// lookup at time — все pure functions без React deps.
//
// Сохраняется ru-локаль форматирование (weekday + month + duration).

import type { Call } from '../api/recording';
import type { CallSpeakerView } from '../api/speakers';

/** [V5.3] Превращает технический speaker_tag (от STT диаризации) в
 *  человекочитаемую подпись:
 *    - "owner"      → "Я"
 *    - "Speaker 0"  → "Голос 1"
 *    - "Speaker 12" → "Голос 13"
 *    - "S0" / "S2"  → "Голос 1" / "Голос 3"
 *    - "Marina"     → "Marina"   (произвольный кастом — оставляем как есть)
 *
 *  Используется во всех UI местах где раньше отображался raw speaker_tag
 *  (SpeakerCard subtitle, SpeakersSection «Подтверждены» подпись, и т.д.).
 *  Маппинг 1-based чтобы юзеру не казалось «нулевой голос». */
export function humanSpeakerLabel(speakerTag: string): string {
  if (!speakerTag) return 'Голос';
  if (speakerTag === 'owner') return 'Я';
  // "Speaker N" (Soniox) → "Голос N+1"
  const m1 = /^Speaker\s+(\d+)$/i.exec(speakerTag);
  if (m1) {
    const n = Number(m1[1]);
    if (Number.isFinite(n) && n >= 0) return `Голос ${n + 1}`;
  }
  // "SN" сокращённое → "Голос N+1"
  const m2 = /^S(\d+)$/.exec(speakerTag);
  if (m2) {
    const n = Number(m2[1]);
    if (Number.isFinite(n) && n >= 0) return `Голос ${n + 1}`;
  }
  // Произвольный кастомный тег — оставляем как есть.
  return speakerTag;
}

/** [V5.3] Короткая версия для avatar-кружков (56×56). Возвращает 1-2 chars:
 *    - "owner"      → "Я"
 *    - "Speaker 0"  → "1"
 *    - "S5"         → "6"
 *    - "Marina"     → "M"  (первая буква)
 *  Когда speaker_tag не numeric (custom display name), берём первую букву —
 *  избегаем «peake»-truncation глитч на длинных строках. */
export function shortSpeakerLabel(speakerTag: string): string {
  if (!speakerTag) return '·';
  if (speakerTag === 'owner') return 'Я';
  const m1 = /^Speaker\s+(\d+)$/i.exec(speakerTag);
  if (m1) {
    const n = Number(m1[1]);
    if (Number.isFinite(n) && n >= 0) return String(n + 1);
  }
  const m2 = /^S(\d+)$/.exec(speakerTag);
  if (m2) {
    const n = Number(m2[1]);
    if (Number.isFinite(n) && n >= 0) return String(n + 1);
  }
  return speakerTag.charAt(0).toUpperCase();
}

export interface CurrentSpeakerInfo {
  tag: string;
  displayName: string;
  colorIdx: number;
}

/** Header meta line: «СРЕДА · 20 МАЯ · 16:04 · 1 МИН 12 СЕК» */
export function formatHeaderMeta(call: Call): string {
  try {
    const d = new Date(call.started_at);
    if (Number.isNaN(d.getTime())) return call.started_at;
    const weekday = d.toLocaleDateString('ru-RU', { weekday: 'long' });
    const date = d.toLocaleDateString('ru-RU', { day: 'numeric', month: 'long' });
    const time = d.toLocaleTimeString('ru-RU', {
      hour: '2-digit',
      minute: '2-digit',
    });
    const parts = [capitalize(weekday), date, time];
    if (call.duration_sec && call.duration_sec > 0) {
      parts.push(humanDuration(call.duration_sec));
    }
    return parts.join(' · ');
  } catch {
    return call.started_at;
  }
}

/** «12 сек» / «5 мин 14 сек» / «1 ч 25 мин» */
export function humanDuration(sec: number): string {
  if (sec < 60) return `${sec} сек`;
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  if (m < 60) {
    return s > 0 ? `${m} мин ${s} сек` : `${m} мин`;
  }
  const h = Math.floor(m / 60);
  const rm = m % 60;
  return rm > 0 ? `${h} ч ${rm} мин` : `${h} ч`;
}

export function capitalize(s: string): string {
  if (!s) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** Fallback title когда LLM не сгенерировал call.title — «Звонок · 20 мая». */
export function simpleDateTitle(call: Call): string {
  try {
    const d = new Date(call.started_at);
    if (Number.isNaN(d.getTime())) return `Звонок ${call.id.slice(0, 8)}`;
    const date = d.toLocaleDateString('ru-RU', {
      day: 'numeric',
      month: 'long',
    });
    return `Звонок · ${date}`;
  } catch {
    return `Звонок ${call.id.slice(0, 8)}`;
  }
}

/** Хэш call_id для стабильного seed waveform'а. */
export function hashCallId(id: string): number {
  let h = 0;
  for (const ch of id) h = (h * 31 + ch.charCodeAt(0)) | 0;
  return Math.abs(h) % 1000;
}

/** Pluralization ru: «1 участник / 2-4 участника / 5+ участников». */
export function pluralParticipants(n: number): string {
  const mod10 = n % 10;
  const mod100 = n % 100;
  if (mod10 === 1 && mod100 !== 11) return 'участник';
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return 'участника';
  return 'участников';
}

/**
 * Найти спикера в момент `currentTime` (sec) в merged-транскрипте.
 * Возвращает `{tag, displayName, colorIdx}` или null если pause/не найден.
 *
 * `colorIdx` — порядок уникальных тегов в merged'е (для стабильной палитры).
 * `displayName` — `contact_display_name` если confirmed, иначе fallback
 * («Я» для owner, тег как есть для прочих).
 *
 * 250ms slack за конец сегмента — smooth transition между блоками.
 */
export function findSpeakerAtTime(
  rawSttJson: string | null,
  speakers: CallSpeakerView[],
  currentTime: number,
): CurrentSpeakerInfo | null {
  if (!rawSttJson || !Number.isFinite(currentTime)) return null;
  try {
    const data = JSON.parse(rawSttJson) as {
      merged?: Array<{
        start?: number;
        end?: number;
        speakerTag?: string;
      }>;
    };
    if (!Array.isArray(data.merged)) return null;
    const tagOrder: string[] = [];
    for (const seg of data.merged) {
      if (typeof seg?.speakerTag !== 'string') continue;
      if (!tagOrder.includes(seg.speakerTag)) tagOrder.push(seg.speakerTag);
    }
    for (const seg of data.merged) {
      const tag = seg?.speakerTag;
      const start = seg?.start ?? 0;
      const end = seg?.end ?? start;
      if (typeof tag !== 'string') continue;
      if (currentTime >= start && currentTime <= end + 0.25) {
        const labelMatch = speakers.find(
          (s) => s.confirmed && s.contact_display_name && s.speaker_tag === tag,
        );
        const displayName =
          labelMatch?.contact_display_name ?? humanSpeakerLabel(tag);
        const colorIdx = tagOrder.indexOf(tag);
        return { tag, displayName, colorIdx: Math.max(0, colorIdx) };
      }
    }
    return null;
  } catch {
    return null;
  }
}
