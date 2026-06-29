// [window] Кастомные кнопки управления окном — macOS-светофор (close/min/max).
// Нативные светофоры скрыты в Rust (set_main_traffic_lights_hidden); рисуем свои:
// скрыты по умолчанию, видны при наведении (App: data-chrome). Глифы — inline SVG
// (✕ / − / две угловые стрелки fullscreen), точь-в-точь нативные, видны на hover.
//
// close → CloseRequested → сворачивание в трей (S9). max → нативный fullscreen
// (отдельный Space). Wrapper — data-tauri-drag-region="deep": угол лого таскает
// окно, кнопки (BUTTON) авто-блокируют drag (Tauri drag.js isClickableElement).

import { getCurrentWindow } from '@tauri-apps/api/window';
import { useI18n } from '../i18n';

interface WindowControlsProps {
  /** Раскрыты (App: data-chrome на .app). */
  open: boolean;
  onOpen: () => void;
  onClose: () => void;
}

const win = () => getCurrentWindow();
const ignore = () => {};

async function toggleFullscreen() {
  const w = win();
  const isFs = await w.isFullscreen().catch(() => false);
  await w.setFullscreen(!isFs).catch(ignore);
}

export function WindowControls({ open, onOpen, onClose }: WindowControlsProps) {
  const { t } = useI18n();
  return (
    <div
      className="win-controls"
      data-tauri-drag-region="deep"
      // [a11y] Скрыты по умолчанию (opacity/pointer-events:none). Убираем из
      // tab-order и из дерева скринридера пока не раскрыты — нативные ⌘W/⌘M
      // остаются доступны с клавиатуры.
      aria-hidden={open ? undefined : true}
      onMouseEnter={onOpen}
      onMouseLeave={onClose}
    >
      <button
        type="button"
        className="wc-btn wc-btn--close"
        aria-label={t('common.winClose')}
        tabIndex={open ? 0 : -1}
        onClick={() => void win().close().catch(ignore)}
      >
        <svg className="wc-glyph" viewBox="0 0 12 12" aria-hidden="true">
          <path
            d="M4 4 L8 8 M8 4 L4 8"
            stroke="currentColor"
            strokeWidth="1.3"
            strokeLinecap="round"
            fill="none"
          />
        </svg>
      </button>
      <button
        type="button"
        className="wc-btn wc-btn--min"
        aria-label={t('common.winMinimize')}
        tabIndex={open ? 0 : -1}
        onClick={() => void win().minimize().catch(ignore)}
      >
        <svg className="wc-glyph" viewBox="0 0 12 12" aria-hidden="true">
          <path
            d="M3.4 6 H8.6"
            stroke="currentColor"
            strokeWidth="1.3"
            strokeLinecap="round"
            fill="none"
          />
        </svg>
      </button>
      <button
        type="button"
        className="wc-btn wc-btn--max"
        aria-label={t('common.winMaximize')}
        tabIndex={open ? 0 : -1}
        onClick={() => void toggleFullscreen().catch(ignore)}
      >
        {/* Нативный «enter fullscreen» — две треугольные стрелки в углы. */}
        <svg className="wc-glyph" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M3.4 3.4 L6.4 3.4 L3.4 6.4 Z M8.6 8.6 L5.6 8.6 L8.6 5.6 Z" fill="currentColor" />
        </svg>
      </button>
    </div>
  );
}
