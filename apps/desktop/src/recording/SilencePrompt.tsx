// [T7/R15] In-app баннер «в записи тишина — остановить?».
//
// Поднимается на backend-событие `recording:silence_prompt` (см.
// `audio/silence_watch` + `commands/silence.rs`). Параллельно уходит нативное
// уведомление — оно нужно когда окно свёрнуто, баннер когда юзер здесь.
//
// Отличие от SuggestBanner: авто-дисмисса нет. Тот предлагает начать запись, и
// пропущенное предложение ничего не стоит; здесь запись уже идёт и через
// N минут остановится сама — гасить вопрос по таймеру значило бы принять
// решение за пользователя (и нарушить SC 2.2.1, у которого таймаут на
// интерактиве требует управления).
//
// A11y:
//   - role="status" + aria-live="polite" — анонс без воровства фокуса.
//   - Обе кнопки с явными aria-label'ами.

import { useCallback, useEffect, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

import { useI18n } from '../i18n';
import { notifyNative } from '../notify';

import { useRecording } from './RecordingContext';

/** Payload `recording:silence_prompt` — `events::RecordingSilencePromptEvent`. */
interface SilencePromptPayload {
  call_id: string;
  silent_for_ms: number;
  /** `null` — настройка `never`: сама запись не остановится. */
  auto_stop_in_ms: number | null;
}

/** Payload `recording:auto_stopped` — `events::RecordingAutoStoppedEvent`. */
interface AutoStoppedPayload {
  call_id: string;
  silent_for_ms: number;
  trimmed_ms: number;
}

const MS_PER_MIN = 60_000;

/** Минуты для текста. Округляем вверх: «через 0 мин» читается как ошибка. */
function toMinutes(ms: number): number {
  return Math.max(1, Math.ceil(ms / MS_PER_MIN));
}

interface SilencePromptProps {
  /** Стоп через тот же обработчик, что рельса и хоткей (`App.onStop`): он
   *  показывает тост «слишком коротко» для отброшенной записи и уводит на
   *  страницу звонка. Прямой `rec.stop()` тихо оставлял бы юзера на месте. */
  onStop: () => Promise<void>;
}

export function SilencePrompt({ onStop }: SilencePromptProps) {
  const { t } = useI18n();
  const rec = useRecording();
  const [pending, setPending] = useState<SilencePromptPayload | null>(null);
  // Слушатели живут всё время и держали бы `t` с первого рендера. Смена языка
  // в настройках даёт новый `t` — без ref нативное уведомление осталось бы на
  // старой локали, пока приложение не перезапустят (баннер-то перерисуется).
  const tRef = useRef(t);
  tRef.current = t;

  // ── Подсказка от backend'а. Баннер и уведомление поднимаются вместе:
  //    первое для активного окна, второе для свёрнутого.
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    void (async () => {
      unlisten = await listen<SilencePromptPayload>(
        'recording:silence_prompt',
        (event) => {
          setPending(event.payload);
          const min = toMinutes(event.payload.silent_for_ms);
          const left = event.payload.auto_stop_in_ms;
          const t = tRef.current;
          void notifyNative(
            t('recording.silenceTitle'),
            left === null
              ? t('recording.silenceBody', { min })
              : t('recording.silenceBodyWithStop', {
                  min,
                  left: toMinutes(left),
                }),
          );
        },
      );
    })();
    return () => {
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Запись остановилась сама: баннер снимаем, а факт проговариваем — иначе
  //    юзер вернётся к компьютеру и не поймёт, кто остановил запись.
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    void (async () => {
      unlisten = await listen<AutoStoppedPayload>(
        'recording:auto_stopped',
        (event) => {
          setPending(null);
          const t = tRef.current;
          void notifyNative(
            t('recording.silenceStoppedTitle'),
            t('recording.silenceStoppedBody', {
              min: toMinutes(event.payload.silent_for_ms),
            }),
          );
        },
      );
    })();
    return () => {
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Вопрос актуален только пока запись идёт. Пауза тоже снимает его:
  //    наблюдатель на паузе сбрасывает счётчик тишины, и обещание «через N мин
  //    остановится сама» стало бы враньём — юзер решил бы, что запись уже
  //    закончилась, пока она стоит на паузе и ждёт его.
  useEffect(() => {
    if (pending && rec.status.kind !== 'recording') {
      setPending(null);
    }
  }, [pending, rec.status.kind]);

  const onStopClick = useCallback(async () => {
    setPending(null);
    try {
      await onStop();
    } catch {
      /* Ошибка всплывёт через rec.error; баннер свою работу сделал. */
    }
  }, [onStop]);

  const onContinue = useCallback(() => {
    setPending(null);
    // Решение владельца: «Продолжить» сбрасывает счётчик тишины целиком —
    // авто-стоп откладывается на полный интервал заново.
    void invoke('snooze_silence_watch').catch((err: unknown) => {
      console.warn('snooze_silence_watch failed', err);
    });
  }, []);

  if (!pending) return null;

  const min = toMinutes(pending.silent_for_ms);
  const body =
    pending.auto_stop_in_ms === null
      ? t('recording.silenceBody', { min })
      : t('recording.silenceBodyWithStop', {
          min,
          left: toMinutes(pending.auto_stop_in_ms),
        });

  return (
    <div
      className="suggest-banner"
      role="status"
      aria-live="polite"
      data-testid="silence-prompt"
    >
      <div className="suggest-banner-body">
        <span className="suggest-banner-title">
          {t('recording.silenceTitle')}
        </span>
        <span className="suggest-banner-text">{body}</span>
      </div>
      <div className="suggest-banner-actions">
        <button
          type="button"
          className="btn btn--primary btn--sm"
          aria-label={t('recording.silenceStop')}
          onClick={() => void onStopClick()}
          disabled={rec.busy}
        >
          {t('recording.silenceStop')}
        </button>
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          aria-label={t('recording.silenceContinue')}
          onClick={onContinue}
        >
          {t('recording.silenceContinue')}
        </button>
      </div>
    </div>
  );
}
