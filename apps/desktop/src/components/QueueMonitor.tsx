// [Q] QueueMonitor — кнопка-индикатор очередей тяжёлых ресурсов в сайдбаре
// (заняла место theme-toggle; тема живёт в Настройки → Оформление) + попап
// «кто в работе / кто в очереди» по whisper/диаризации/LLM — в духе попапа
// утилизации Claude Code.
//
// Данные: useQueueState (сверху, из App) — снапшоты `queue:state`. Title
// звонка резолвится по call_id из списка recent; fallback — короткий id.
// A11y: статусы текстом (не только цветом), aria-label на кнопке,
// native Dropdown (esc / клик-мимо закрывают).

import { useI18n } from '../i18n';
import type { Call } from '../api/recording';
import type { QueueResourceId, QueueResourceState, QueueState } from '../api/queue';
import { Dropdown, IconBtn } from '../ui';

interface QueueMonitorProps {
  queue: QueueState | null;
  /** Для резолва call_id → title (recent из App). */
  calls: Call[];
  iconSize?: number;
}

const RES_ORDER: QueueResourceId[] = ['stt', 'diarization', 'llm'];

/** Дедуп waiting-записей по call_id (mic+system дорожки STT одного звонка). */
function dedupWaiting(
  waiting: QueueResourceState['waiting'],
): { call_id: string | null; count: number }[] {
  const out: { call_id: string | null; count: number }[] = [];
  for (const w of waiting) {
    const last = out.find((o) => o.call_id === w.call_id);
    if (last) {
      last.count += 1;
    } else {
      out.push({ call_id: w.call_id, count: 1 });
    }
  }
  return out;
}

export function QueueMonitor({ queue, calls, iconSize = 16 }: QueueMonitorProps) {
  const { t } = useI18n();

  const titleFor = (callId: string | null): string => {
    if (callId == null) return t('queue.systemTask');
    const call = calls.find((c) => c.id === callId);
    return call?.title ?? callId.slice(0, 8);
  };

  const resLabel = (id: QueueResourceId): string => {
    switch (id) {
      case 'stt':
        return t('queue.res.stt');
      case 'diarization':
        return t('queue.res.diarization');
      case 'llm':
        return t('queue.res.llm');
    }
  };

  const resources: QueueResourceState[] = RES_ORDER.map(
    (id) =>
      queue?.resources.find((r) => r.id === id) ?? { id, busy: null, waiting: [] },
  );
  const anyActive = resources.some((r) => r.busy != null || r.waiting.length > 0);

  return (
    <Dropdown
      up
      align="right"
      width={272}
      trigger={({ toggle }) => (
        <span style={{ position: 'relative', display: 'inline-flex' }}>
          <IconBtn
            icon="cpu"
            size="sm"
            iconSize={iconSize}
            label={t('queue.monitor')}
            title={t('queue.monitor')}
            onClick={toggle}
          />
          {anyActive && (
            <span
              className="dot dot--pulse"
              aria-hidden="true"
              style={{
                position: 'absolute',
                top: 3,
                right: 3,
                background: 'var(--accent)',
              }}
            />
          )}
        </span>
      )}
    >
      <div role="list" aria-label={t('queue.monitor')} style={{ padding: '2px 0' }}>
        {resources.map((r) => {
          const waiting = dedupWaiting(r.waiting);
          return (
            <div key={r.id} role="listitem" style={{ padding: '4px 10px 6px' }}>
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  fontSize: 12.5,
                  fontWeight: 600,
                  color: 'var(--text)',
                }}
              >
                <span style={{ flex: 1, minWidth: 0 }}>{resLabel(r.id)}</span>
                <span
                  className="mono"
                  style={{
                    fontSize: 10.5,
                    letterSpacing: '0.04em',
                    color: r.busy ? 'var(--accent-text)' : 'var(--text-faint)',
                  }}
                >
                  {r.busy ? t('queue.busy') : t('queue.free')}
                </span>
              </div>
              {r.busy && (
                <div
                  className="u-trunc"
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    marginTop: 3,
                    fontSize: 12,
                    color: 'var(--text-2)',
                  }}
                  title={titleFor(r.busy.call_id)}
                >
                  <span
                    className="dot dot--pulse"
                    aria-hidden="true"
                    style={{ background: 'var(--accent)', flex: '0 0 auto' }}
                  />
                  <span className="u-trunc">{titleFor(r.busy.call_id)}</span>
                </div>
              )}
              {waiting.map((w, i) => (
                <div
                  key={`${w.call_id ?? 'sys'}-${i}`}
                  className="u-trunc"
                  style={{
                    marginTop: 2,
                    paddingLeft: 14,
                    fontSize: 11.5,
                    color: 'var(--text-3)',
                  }}
                  title={titleFor(w.call_id)}
                >
                  <span className="mono" style={{ fontSize: 10.5, marginRight: 5 }}>
                    {i + 1}.
                  </span>
                  {titleFor(w.call_id)}
                  {w.count > 1 && (
                    <span className="mono" style={{ fontSize: 10.5, color: 'var(--text-faint)' }}>
                      {' '}
                      ×{w.count}
                    </span>
                  )}
                </div>
              ))}
              {!r.busy && waiting.length === 0 && (
                <div style={{ marginTop: 2, fontSize: 11.5, color: 'var(--text-faint)' }}>
                  {t('queue.empty')}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </Dropdown>
  );
}
