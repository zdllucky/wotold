import React from 'react';
import ReactDOM from 'react-dom/client';
import './dev-tauri-mock';
import { App } from './App';
import { RecordingWidgetApp } from './recording/RecordingWidgetApp';
import './styles/fonts.css';
import './styles/tokens.css';
import './styles/wotold.css';
import './styles/global.css';
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
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    {isRecordingWidget ? <RecordingWidgetApp /> : <App />}
  </React.StrictMode>,
);
