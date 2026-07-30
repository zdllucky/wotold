// Готовность локального движка — одно состояние на всё приложение.
//
// Раньше каждый экран слушал `model:progress`/`model:done` сам и держал свою
// картину мира: таблица моделей в настройках, карточка эмбеддера в спикерах,
// шаг онбординга. Отсюда и расхождения — один экран показывал прогресс,
// другой в это же время «не установлено».
//
// Здесь единственная подписка и единственный источник правды: снимок готовности
// из `local_engine_readiness` + живой агрегированный прогресс докачки.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { LocalEngineReadiness, ModelProgressEvent } from '@wotold/contracts';
import { localEngineEnsureRequired, localEngineReadiness } from '../../api/local-engine';
import { humanError } from '../../api/errors';
import { useI18n } from '../../i18n';

/** Суммарный прогресс докачки обязательных модулей. */
export interface ReadinessAggregate {
  pct: number;
  doneBytes: number;
  totalBytes: number;
}

export interface ReadinessState {
  /** `null` пока снимок не получен (или движка нет на этой платформе). */
  readiness: LocalEngineReadiness | null;
  /** Идёт докачка. */
  downloading: boolean;
  aggregate: ReadinessAggregate | null;
  /** id модулей, по которым прогресс уже шёл — то есть качающихся сейчас. */
  downloadingIds: Set<string>;
  /** Запустить докачку недостающего. Идемпотентно, single-flight на бэкенде. */
  ensure: () => void;
  /** Текст последней ошибки докачки — баннер показывает «Повторить». */
  lastError: string | null;
}

const FALLBACK: ReadinessState = {
  readiness: null,
  downloading: false,
  aggregate: null,
  downloadingIds: new Set(),
  ensure: () => {},
  lastError: null,
};

const Ctx = createContext<ReadinessState>(FALLBACK);

/** Состояние готовности движка. Вне провайдера — безопасная заглушка. */
export function useReadiness(): ReadinessState {
  return useContext(Ctx);
}

/** Не хватает ли обязательных модулей (и размер уже выбран). */
export function isMissingModules(state: ReadinessState): boolean {
  const r = state.readiness;
  return !!r && !r.ready && r.preset !== null;
}

/** Размер движка ещё не выбран — качать нечего, ведём в настройки. */
export function needsPresetChoice(state: ReadinessState): boolean {
  const r = state.readiness;
  return !!r && !r.ready && r.preset === null;
}

/** Событие `model:done` — union по `status`. */
type ModelDoneEvent =
  | { id: string; status: 'ok' | 'already_present' }
  | { id: string; status: 'verify_failed'; expected: string; got: string }
  | { id: string; status: 'io_error'; message: string };

export function ReadinessProvider({ children }: { children: ReactNode }) {
  const { t } = useI18n();
  const [readiness, setReadiness] = useState<LocalEngineReadiness | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [lastError, setLastError] = useState<string | null>(null);
  /** bytes_done по каждому модулю, который сейчас качается или уже докачан. */
  const [doneByModel, setDoneByModel] = useState<Record<string, number>>({});
  /** Размеры модулей из снимка — чтобы знать знаменатель прогресса. */
  const sizes = useRef<Record<string, number>>({});
  // `t` не стабилен между рендерами, а подписки на события обязаны жить одну
  // на сессию. С `t` в зависимостях эффект пересоздавал листенеры на каждый
  // рендер, и события, пришедшие между отпиской и асинхронной переподпиской,
  // терялись — прогресс скачивания вставал на месте.
  const tRef = useRef(t);
  tRef.current = t;

  const refresh = useCallback(async () => {
    try {
      const r = await localEngineReadiness();
      sizes.current = Object.fromEntries(r.missing.map((m) => [m.id, m.bytes_total]));
      setReadiness(r);
      return r;
    } catch (e) {
      // Снимок не получен — оставляем состояние неизвестным (`null`), а не
      // объявляем движок готовым. Команды движка есть только под macOS (R9),
      // и там неизвестность равна «баннера нет»; но та же ветка ловит и
      // обычные сбои вроде занятой базы, а «готов» на такой ошибке скрывал бы
      // единственную точку входа в докачку до перезапуска приложения.
      console.warn('readiness snapshot failed', e);
      setReadiness(null);
      return null;
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    // [Review HIGH-2] cancelled-guard: listen() — async IPC, и cleanup до его
    // резолва оставлял бы listener'ы висеть до конца сессии.
    let cancelled = false;
    let unReadiness: UnlistenFn | undefined;
    let unProgress: UnlistenFn | undefined;
    let unDone: UnlistenFn | undefined;
    (async () => {
      unReadiness = await listen<LocalEngineReadiness>('readiness:changed', (e) => {
        sizes.current = Object.fromEntries(e.payload.missing.map((m) => [m.id, m.bytes_total]));
        setReadiness(e.payload);
        if (e.payload.ready) {
          setDownloading(false);
          setDoneByModel({});
        }
      });
      unProgress = await listen<ModelProgressEvent>('model:progress', (e) => {
        const { id, bytes_done, bytes_total } = e.payload;
        if (bytes_total > 0) sizes.current[id] = bytes_total;
        setDoneByModel((prev) => ({ ...prev, [id]: bytes_done }));
      });
      unDone = await listen<ModelDoneEvent>('model:done', (e) => {
        const payload = e.payload;
        // Докачанный модуль засчитываем целиком: последнее событие прогресса
        // приходит на границе килобайтного шага, а не ровно в конце файла.
        if (payload.status === 'ok' || payload.status === 'already_present') {
          const full = sizes.current[payload.id];
          if (full != null) {
            setDoneByModel((prev) => ({ ...prev, [payload.id]: full }));
          }
        } else if (payload.status === 'verify_failed') {
          setLastError(tRef.current('readiness.verifyFailed'));
        } else if (payload.status === 'io_error') {
          setLastError(payload.message);
        }
      });
      if (cancelled) {
        unReadiness?.();
        unProgress?.();
        unDone?.();
      }
    })();
    return () => {
      cancelled = true;
      unReadiness?.();
      unProgress?.();
      unDone?.();
    };
  }, []);

  const ensure = useCallback(() => {
    setLastError(null);
    setDownloading(true);
    setDoneByModel({});
    void localEngineEnsureRequired()
      .catch((e) => setLastError(humanError(e, t)))
      .finally(() => {
        setDownloading(false);
        void refresh();
      });
  }, [refresh, t]);

  const aggregate = useMemo<ReadinessAggregate | null>(() => {
    if (!readiness || readiness.ready) return null;
    const totalBytes = readiness.missing_bytes_total;
    if (totalBytes <= 0) return null;
    // Считаем только по тем модулям, что ещё в списке недостающих: докачанные
    // уходят из снимка, и их байты уже не участвуют в знаменателе.
    const missingIds = new Set(readiness.missing.map((m) => m.id));
    const doneBytes = Object.entries(doneByModel)
      .filter(([id]) => missingIds.has(id))
      .reduce((sum, [, bytes]) => sum + bytes, 0);
    const clamped = Math.min(doneBytes, totalBytes);
    return {
      doneBytes: clamped,
      totalBytes,
      pct: Math.round((clamped / totalBytes) * 100),
    };
  }, [readiness, doneByModel]);

  const downloadingIds = useMemo(() => new Set(Object.keys(doneByModel)), [doneByModel]);

  const value = useMemo<ReadinessState>(
    () => ({ readiness, downloading, aggregate, downloadingIds, ensure, lastError }),
    [readiness, downloading, aggregate, downloadingIds, ensure, lastError],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
