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

interface ToastItem {
  id: number;
  message: string;
  tone: ToastTone;
}

interface ToastOptions {
  message: string;
  tone?: ToastTone;
  /** ms до авто-исчезновения; 0 = не исчезать сам. */
  duration?: number;
}

interface ToastApi {
  show: (opts: ToastOptions) => void;
}

const ToastContext = createContext<ToastApi | null>(null);

const DEFAULT_DURATION = 4500;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const idRef = useRef(0);
  const timersRef = useRef<number[]>([]);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const show = useCallback(
    ({ message, tone = 'info', duration = DEFAULT_DURATION }: ToastOptions) => {
      idRef.current += 1;
      const id = idRef.current;
      setToasts((prev) => [...prev, { id, message, tone }]);
      if (duration > 0) {
        timersRef.current.push(window.setTimeout(() => dismiss(id), duration));
      }
    },
    [dismiss],
  );

  // Чистим все авто-dismiss таймеры на unmount (нет setState на мёртвом провайдере).
  useEffect(() => {
    const timers = timersRef.current;
    return () => {
      for (const h of timers) window.clearTimeout(h);
    };
  }, []);

  const api = useMemo(() => ({ show }), [show]);

  return (
    <ToastContext.Provider value={api}>
      {children}
      <Toaster toasts={toasts} onDismiss={dismiss} />
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
}: {
  toasts: ToastItem[];
  onDismiss: (id: number) => void;
}) {
  const { t } = useI18n();
  // Контейнер всегда смонтирован (стабильный live-region, иначе SR пропускает
  // первое сообщение). aria-live на контейнере, не на каждом тосте.
  return (
    <div className="toaster" aria-live="polite" aria-atomic="false">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast toast--${toast.tone}`} role="status">
          <span className="toast-msg">{toast.message}</span>
          <button
            type="button"
            className="toast-close"
            aria-label={t('common.dismiss')}
            onClick={() => onDismiss(toast.id)}
          >
            <Icon name="x" size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
