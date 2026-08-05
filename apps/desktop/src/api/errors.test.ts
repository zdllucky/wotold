// @vitest-environment node
import { describe, expect, test } from 'vitest';
import { humanError } from './errors';
import { ru } from '../i18n/ru';
import type { TranslationKey } from '../i18n';

// [TD-25] Строки уехали в словарь — тест берёт настоящий русский словарь,
// чтобы проверять реальные тексты, а не заглушки. Заодно это ловит
// рассинхрон ключей: отсутствующий ключ вернётся как сам ключ и матч упадёт.
const t = (key: TranslationKey, params?: Record<string, string | number>): string => {
  const value = key
    .split('.')
    .reduce<unknown>((acc, part) => (acc as Record<string, unknown>)?.[part], ru);
  const str = typeof value === 'string' ? value : key;
  return params
    ? Object.entries(params).reduce((out, [k, v]) => out.replaceAll(`{${k}}`, String(v)), str)
    : str;
};

describe('humanError', () => {
  test('network refused', () => {
    expect(humanError(new Error('ECONNREFUSED'), t)).toMatch(/нет соединения/i);
  });
  // [Bug-fix] Локальный engine error patterns — должны попадать в local-
  // specific human messages, а не в "Сервис временно занят".
  test('local_llm_timeout → не успела ответить', () => {
    expect(humanError('local_engine_llm_failed: provider: local_llm_timeout', t)).toMatch(
      /не успела ответить/i,
    );
  });
  test('local_engine_model_missing → понятный текст', () => {
    expect(
      humanError('local_engine_model_missing: модель qwen25-3b не установлена', t),
    ).toMatch(/не установлена/i);
  });
  // Припаркованный звонок: текст обещает автоматическую обработку после
  // докачки, а не ручную установку — иначе пользователь пойдёт искать кнопку,
  // которую нажимать не нужно.
  test('local_engine_not_ready → не хватает софта + обработается сам', () => {
    const msg = 'local_engine_not_ready: не хватает модулей: silero-vad-v5, voice-embedder';
    expect(humanError(msg, t)).toMatch(/не хватает софта/i);
    expect(humanError(msg, t)).toMatch(/скачать/i);
  });
  test('local_engine_not_ready НЕ перехватывается generic model_missing', () => {
    const out = humanError('local_engine_not_ready: не хватает модулей: whisper-small', t);
    expect(out).not.toMatch(/локальная модель не установлена/i);
  });
  test('local_engine_model_tampered → про целостность, а не про отсутствие', () => {
    const out = humanError('local_engine_model_tampered: файл whisper-small', t);
    expect(out).toMatch(/целостност/i);
  });
  test('local_engine_preset_not_set → понятный текст', () => {
    expect(
      humanError('local_engine_preset_not_set: выберите Light/Balanced/Quality', t),
    ).toMatch(/preset локального движка/i);
  });
  test('generic local_engine_llm_failed без timeout', () => {
    expect(humanError('local_engine_llm_failed: provider: gbnf parse error', t)).toMatch(
      /не справилась/i,
    );
  });
  test('mic permission', () => {
    expect(humanError(new Error('Failed: NSMicrophoneUsageDescription'), t)).toMatch(
      /микрофон/i,
    );
  });
  // [perm-usage] Коды pre-check'а из `start_recording`. Общий паттерн
  // `permission denied` стоял выше конкретных и съедал оба — вместо «нет
  // доступа к микрофону» приезжало безадресное «нет разрешения системы»,
  // и пользователь не знал, какое из двух разрешений чинить.
  test('permission denied: microphone → адресное сообщение про микрофон', () => {
    expect(humanError('permission denied: microphone', t)).toMatch(/микрофон/i);
  });
  test('permission denied: screen capture → про системный звук', () => {
    expect(humanError('permission denied: screen capture', t)).toMatch(/системного звука/i);
  });
  // Сайдкар умер, не прислав события — так выглядел TCC-SIGABRT из-за
  // отсутствующего NSMicrophoneUsageDescription. Раньше сырая английская
  // строка ехала в UI как есть.
  test('permissions sidecar terminated → переведённое сообщение', () => {
    const out = humanError('permissions sidecar terminated: signal 6', t);
    expect(out).toMatch(/не удалось проверить разрешения/i);
    expect(out).not.toMatch(/terminated/i);
  });
  // Таймаут и смерть сайдкара — разные отказы: первый обычно значит, что
  // системный диалог остался за окном, второй — что сборка сломана.
  test('permissions probe timed out → про незамеченный диалог, а не про поломку', () => {
    const out = humanError('permissions probe timed out (120s)', t);
    expect(out).toMatch(/без ответа/i);
    expect(out).not.toMatch(/переустанови/i);
  });
  test('permissions probe timed out не уезжает в generic timeout', () => {
    expect(humanError('permissions probe timed out (5s)', t)).not.toMatch(/интернет/i);
  });
  // Неудавшийся сброс TCC не должен превращаться в «нет доступа к микрофону»:
  // имя сервиса в тексте — Microphone, и мик-паттерн его подхватывал.
  test('permission reset failed → про сброс, а не про доступ к микрофону', () => {
    const out = humanError('permission reset failed: Microphone (exit status: 1)', t);
    expect(out).toMatch(/сбросить доступ/i);
    expect(out).not.toMatch(/перезапусти приложение/i);
  });
  test('permission reset failed: ScreenCapture → тот же текст', () => {
    expect(humanError('permission reset failed: ScreenCapture (exit status: 1)', t)).toMatch(
      /сбросить доступ/i,
    );
  });
  // Хвост stderr не должен уводить сообщение в чужую ветку словаря.
  test('stderr про микрофон не перебивает сообщение о смерти сайдкара', () => {
    expect(
      humanError('permissions sidecar terminated: signal 6; microphone init failed', t),
    ).toMatch(/не удалось проверить разрешения/i);
  });
  test('disk full', () => {
    expect(humanError(new Error('ENOSPC: no space left'), t)).toMatch(/места на диске/i);
  });
  test('sqlite busy', () => {
    expect(humanError(new Error('database is locked'), t)).toMatch(/база данных занята/i);
  });
  test('cancelled', () => {
    expect(humanError(new Error('aborted by user'), t)).toMatch(/отменена/i);
  });
  test('unknown passes through truncated', () => {
    const long = 'X'.repeat(200);
    const out = humanError(new Error(long), t);
    expect(out.length).toBeLessThanOrEqual(165);
    expect(out).toContain('XXXXX');
  });
  test('null/undefined safe', () => {
    expect(humanError(null, t)).toBe('Неизвестная ошибка');
    expect(humanError(undefined, t)).toBe('Неизвестная ошибка');
  });

  // [P2.2] Closing local_engine error coverage gaps. Без этих pattern'ов
  // юзер видел бы raw token «local_engine_recap_persist: ...» в баннере
  // failed_reason.

  test('local_engine_no_app_handle → внутренняя ошибка + перезапуск', () => {
    expect(
      humanError('local_engine_no_app_handle: pipeline requires Tauri runtime', t),
    ).toMatch(/внутренняя ошибка/i);
    expect(
      humanError('local_engine_no_app_handle: pipeline requires Tauri runtime', t),
    ).toMatch(/перезапусти/i);
  });

  test('local_engine_transcript_read → переобработать', () => {
    expect(humanError('local_engine_transcript_read: enoent', t)).toMatch(
      /прочитать транскрипт/i,
    );
    expect(humanError('local_engine_transcript_read: enoent', t)).toMatch(/переобработать/i);
  });

  test('local_engine_stt_failed (mic) → проверь модели', () => {
    expect(humanError('local_engine_stt_failed (mic): sherpa-onnx panic', t)).toMatch(
      /транскрипция не справилась/i,
    );
    expect(humanError('local_engine_stt_failed (mic): sherpa-onnx panic', t)).toMatch(
      /модели установлены/i,
    );
  });

  test('local_engine_recap_persist → сгенерировано но не сохранилось', () => {
    expect(humanError('local_engine_recap_persist: sqlite write', t)).toMatch(
      /сгенерировано, но не сохранилось/i,
    );
  });

  // Регрессия: новые pattern'ы должны идти ДО generic local_engine_llm_failed.
  // Verify что persist/stt_failed/transcript_read не cabbed под "не справилась
  // с задачей".
  test('local_engine_recap_persist НЕ catches как llm_failed', () => {
    const out = humanError('local_engine_recap_persist: sqlite', t);
    expect(out).not.toMatch(/не справилась с задачей/i);
  });
});
