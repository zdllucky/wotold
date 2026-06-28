import React from 'react';
import ReactDOM from 'react-dom/client';
import './dev-tauri-mock';
import { App } from './App';
import { RecordingWidgetApp } from './recording/RecordingWidgetApp';
import './styles/fonts.css';
import './styles/tokens.css';
// [B18.0] legacy-tokens shim — Atelier-имена → uikit (удалить в B18.6d, остался
// только для DesignSystemPage до B18.6c).
import './styles/legacy-tokens.css';
import './styles/global.css';
// [B18.0] Wotold v2 component-слой (uikit) — примитивы.
import './styles/wk.css';
// [B18.6b] App component-классы (uikit) — порт wotold.css used-классов.
import './styles/components.css';
import './ui/ui.css';

const root = document.getElementById('root');
if (!root) throw new Error('root element missing');

// [W4] Hash-route dispatch — `tauri.conf.json` opens the floating widget as
// `index.html#recording-widget`. We render an entirely separate React tree
// for that window: independent `<RecordingProvider>` driven by the same
// Tauri events / commands, so the widget and the main window stay
// consistent without sharing memory (different webviews).
const isRecordingWidget =
  typeof window !== 'undefined' &&
  window.location.hash === '#recording-widget';
if (isRecordingWidget) {
  document.documentElement.classList.add('recording-widget');
  // [S8] Belt-and-suspenders: ensure html/body/#root are visually transparent
  // even if CSS cascade misfires (Vite HMR injects styles late in dev). Inline
  // style wins без !important борьбы.
  const transparentize = (el: HTMLElement | null) => {
    if (!el) return;
    el.style.background = 'transparent';
    el.style.backgroundColor = 'transparent';
  };
  transparentize(document.documentElement);
  transparentize(document.body);
  transparentize(root);
  // Drag-region на body чтобы вся область окна (включая transparent margin
  // вокруг pill) ловила mousedown и запускала NSWindow drag. Кнопки
  // .rec-float-actions имеют data-tauri-drag-region="false" — Tauri 2
  // уважает override через closest(), поэтому pause/stop НЕ инициируют drag.
  document.body.setAttribute('data-tauri-drag-region', '');
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    {isRecordingWidget ? <RecordingWidgetApp /> : <App />}
  </React.StrictMode>,
);
