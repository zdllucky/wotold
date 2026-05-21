// [W4] Root for the secondary `recording-widget` Tauri window.
//
// This is a separate React tree from `<App />` (different webview / different
// hash route). We wrap `RecFloat` in the minimum providers it needs:
//   - `I18nProvider` for labels (paused/recording, aria),
//   - `ThemeProvider` so the widget honours light/dark/accent settings,
//   - `RecordingProvider` for its own status mirror (sourced from the same
//     Rust state via `getRecordingState` + Tauri events, so it stays in sync
//     with the main window automatically).
//
// On `idle` we auto-hide the widget — even if the main window forgot to.

import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

import { I18nProvider } from '../i18n';
import { ThemeProvider } from '../theme/useTheme';

import { RecFloat } from './RecFloat';
import { RecordingProvider, useRecording } from './RecordingContext';

function AutoHideOnIdle() {
  const rec = useRecording();
  useEffect(() => {
    if (rec.status.kind !== 'idle') return;
    // The Rust command is idempotent — safe to call even if the widget is
    // already hidden (e.g. just opened with no active recording).
    void invoke('hide_recording_widget').catch((e) => {
      console.warn('hide_recording_widget failed', e);
    });
  }, [rec.status.kind]);
  return null;
}

export function RecordingWidgetApp() {
  return (
    <I18nProvider>
      <ThemeProvider>
        <RecordingProvider>
          <AutoHideOnIdle />
          <RecFloat />
        </RecordingProvider>
      </ThemeProvider>
    </I18nProvider>
  );
}
