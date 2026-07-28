// [ui] Лёгкая toast-система — транзиентные сообщения в правом-нижнем углу.
// Без npm-библиотеки: context + очередь + авто-dismiss. Стили из .suggest-banner
// паттерна (components.css .toaster/.toast). Тоны = цвет левого бордера.

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
import { Icon } from './Icon';
import { useI18n } from '../i18n';

type ToastTone = 'info' | 'success' | 'warn' | 'danger';

/**
 * Действие в тосте. Одно и только одно: тост — не диалог, второй кнопкой он
 * начинает требовать решения, а не предлагать его.
 */
interface ToastAction {
  label: string;
  onClick: () => void;
}

interface ToastItem {
  id: number;
  message: string;
  tone: ToastTone;
  /** ms до авто-исчезновения; 0 = sticky (для pause/resume). */
  duration: number;
  action?: ToastAction;
}

interface ToastOptions {
  message: string;
  tone?: ToastTone;
  /** ms до авто-исчезновения; 0 = не исчезать сам. */
  duration?: number;
  /**
   * Кнопка действия. Тост с действием по умолчанию становится sticky:
   * предложение, исчезающее через 4.5 секунды, — это предложение, которое
   * пользователь не успел прочесть. Явный `duration` перебивает.
   */
  action?: ToastAction;
}

interface ToastApi {
  show: (opts: ToastOptions) => void;
}

const ToastContext = createContext<ToastApi | null>(null);

const DEFAULT_DURATION = 4500;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const idRef = useRef(0);
  // id → timer handle. Удаляем по dismiss/fire — массив не растёт бесконечно.
  const timersRef = useRef<Map<number, number>>(new Map());
  // Зеркало текущих тостов для resume (re-arm после паузы по hover/focus).
  const toastsRef = useRef<ToastItem[]>([]);
  toastsRef.current = toasts;

  const clearTimer = useCallback((id: number) => {
    const h = timersRef.current.get(id);
    if (h !== undefined) {
      window.clearTimeout(h);
      timersRef.current.delete(id);
    }
  }, []);

  const dismiss = useCallback(
    (id: number) => {
      clearTimer(id);
      setToasts((prev) => prev.filter((t) => t.id !== id));
    },
    [clearTimer],
  );

  const armTimer = useCallback(
    (id: number, duration: number) => {
      if (duration <= 0) return;
      clearTimer(id);
      timersRef.current.set(id, window.setTimeout(() => dismiss(id), duration));
    },
    [clearTimer, dismiss],
  );

  const show = useCallback(
    ({ message, tone = 'info', action, duration }: ToastOptions) => {
      idRef.current += 1;
      const id = idRef.current;
      // Тост с действием ждёт решения, а не отсчитывает секунды.
      const ttl = duration ?? (action ? 0 : DEFAULT_DURATION);
      setToasts((prev) => [...prev, { id, message, tone, duration: ttl, action }]);
      armTimer(id, ttl);
    },
    [armTimer],
  );

  // [a11y SC 2.2.1] Пауза авто-dismiss пока курсор/фокус на тостере, чтобы
  // пользователь успел прочитать/среагировать; re-arm когда уходит.
  const pauseAll = useCallback(() => {
    for (const h of timersRef.current.values()) window.clearTimeout(h);
    timersRef.current.clear();
  }, []);

  const resumeAll = useCallback(() => {
    for (const item of toastsRef.current) armTimer(item.id, item.duration);
  }, [armTimer]);

  // Чистим все авто-dismiss таймеры на unmount (нет setState на мёртвом провайдере).
  useEffect(() => {
    const timers = timersRef.current;
    return () => {
      for (const h of timers.values()) window.clearTimeout(h);
      timers.clear();
    };
  }, []);

  const api = useMemo(() => ({ show }), [show]);

  return (
    <ToastContext.Provider value={api}>
      {children}
      <Toaster
        toasts={toasts}
        onDismiss={dismiss}
        onPause={pauseAll}
        onResume={resumeAll}
      />
    </ToastContext.Provider>
  );
}

export function useToast(): ToastApi {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error('useToast must be used within <ToastProvider>');
  return ctx;
}

function Toaster({
  toasts,
  onDismiss,
  onPause,
  onResume,
}: {
  toasts: ToastItem[];
  onDismiss: (id: number) => void;
  onPause: () => void;
  onResume: () => void;
}) {
  const { t } = useI18n();
  // Контейнер всегда смонтирован (стабильный live-region, иначе SR пропускает
  // первое сообщение). aria-live на контейнере, НЕ role=status на каждом тосте
  // (иначе VoiceOver/NVDA дублируют объявление — вложенные live-region'ы).
  return (
    <div
      className="toaster"
      aria-live="polite"
      aria-atomic="false"
      onMouseEnter={onPause}
      onMouseLeave={onResume}
      onFocus={onPause}
      onBlur={onResume}
    >
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast toast--${toast.tone}`}>
          <span className="toast-msg">{toast.message}</span>
          {toast.action && (
            <button
              type="button"
              className="toast-action"
              onClick={() => {
                // Тост уходит сразу: действие запущено, повторное нажатие
                // по той же кнопке ничего хорошего не даст.
                onDismiss(toast.id);
                toast.action?.onClick();
              }}
            >
              {toast.action.label}
            </button>
          )}
          <button
            type="button"
            className="toast-close"
            // [a11y SC 4.1.2] «Закрыть» без контекста: при нескольких тостах
            // подряд скринридер зачитывает три одинаковые кнопки. Имя должно
            // говорить, что именно закрывается.
            aria-label={t('common.dismissToast', { message: toast.message })}
            onClick={() => onDismiss(toast.id)}
          >
            <Icon name="x" size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
