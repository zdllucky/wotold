// [window] Кастомные кнопки управления окном — macOS-светофор (close/min/max).
// Нативные светофоры скрыты в Rust (lib.rs hide_main_window_buttons); рисуем
// свои: скрыты по умолчанию, видны при наведении (App управляет через
// data-chrome + hover-зону самого wrapper'а). Glyph ×/−/+ — CSS на hover группы.
//
// close → CloseRequested → сворачивание в трей (S9, как нативный красный).

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

export function WindowControls({ open, onOpen, onClose }: WindowControlsProps) {
  const { t } = useI18n();
  return (
    <div
      className="win-controls"
      // [a11y] Скрыты по умолчанию (opacity/pointer-events:none). Убираем из
      // tab-order и из дерева скринридера пока не раскрыты — иначе клавиатура
      // ловила бы 3 невидимых кнопки. ⌘W/⌘M остаются нативными.
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
      />
      <button
        type="button"
        className="wc-btn wc-btn--min"
        aria-label={t('common.winMinimize')}
        tabIndex={open ? 0 : -1}
        onClick={() => void win().minimize().catch(ignore)}
      />
      <button
        type="button"
        className="wc-btn wc-btn--max"
        aria-label={t('common.winMaximize')}
        tabIndex={open ? 0 : -1}
        onClick={() => void win().toggleMaximize().catch(ignore)}
      />
    </div>
  );
}
