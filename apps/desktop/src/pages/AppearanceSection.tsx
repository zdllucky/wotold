// [B17] Appearance section — theme + accent picker. Реализует "Внешний вид"
// раздел из docs/design/atelier-v2/README.md → "Theme switching". Использует
// useTheme() — persist через api/settings (UI_THEME, UI_ACCENT).

import { useTheme, type Accent, type Theme } from '../theme/useTheme';

const THEME_OPTIONS: Array<{ id: Theme; label: string }> = [
  { id: 'light', label: 'Светлая' },
  { id: 'dark', label: 'Тёмная' },
  { id: 'system', label: 'Системная' },
];

const ACCENT_OPTIONS: Array<{ id: Accent; label: string }> = [
  { id: 'bordeaux', label: 'Бордо' },
  { id: 'persian', label: 'Кобальт' },
  { id: 'ink', label: 'Графит' },
];

export function AppearanceSection() {
  const { theme, setTheme, accent, setAccent } = useTheme();

  return (
    <div>
      <div className="field" style={{ marginBottom: 24 }}>
        <label className="field-label">Тема</label>
        <div style={{ display: 'flex', gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
          {THEME_OPTIONS.map((opt) => (
            <button
              key={opt.id}
              type="button"
              className={`btn ${theme === opt.id ? 'btn--primary' : 'btn--ghost'}`}
              onClick={() => setTheme(opt.id)}
              aria-pressed={theme === opt.id}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      <div className="field">
        <label className="field-label">Акцентный цвет</label>
        <div style={{ display: 'flex', gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
          {ACCENT_OPTIONS.map((opt) => (
            <button
              key={opt.id}
              type="button"
              className={`btn ${accent === opt.id ? 'btn--primary' : 'btn--ghost'}`}
              onClick={() => setAccent(opt.id)}
              aria-pressed={accent === opt.id}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
