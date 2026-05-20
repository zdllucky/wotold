// #48 (M7.5 follow-up): прогресс-бар использования квоты.
//
// Lightweight DS-компонент: один track + filled portion + лейбл "used/limit".
// Цвет filled зависит от percent: ok | warning >=75% | danger >=95%.

import './ui.css';

type Tone = 'ok' | 'warning' | 'danger';

interface UsageBarProps {
  label: string;
  used: number;
  limit: number;
  /** Кастомный formatter для значений (e.g. '120 сек', '5,000 токенов'). */
  format?: (v: number) => string;
}

function pickTone(pct: number): Tone {
  if (pct >= 95) return 'danger';
  if (pct >= 75) return 'warning';
  return 'ok';
}

export function UsageBar({ label, used, limit, format }: UsageBarProps) {
  const fmt = format ?? ((v: number) => v.toLocaleString('ru-RU'));
  const safeLimit = limit > 0 ? limit : 0;
  const pct = safeLimit === 0 ? 0 : Math.min(100, Math.round((used / safeLimit) * 100));
  const tone = pickTone(pct);

  return (
    <div className="ds-usagebar" data-tone={tone}>
      <div className="ds-usagebar-header">
        <span className="ds-usagebar-label">{label}</span>
        <span className="ds-usagebar-values">
          {safeLimit === 0 ? (
            <span title="лимит не настроен">{fmt(used)} / ∞</span>
          ) : (
            <>
              {fmt(used)} / {fmt(safeLimit)}{' '}
              <span className="ds-usagebar-pct">({pct}%)</span>
            </>
          )}
        </span>
      </div>
      <div className="ds-usagebar-track" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label={label}>
        <div className="ds-usagebar-fill" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}
