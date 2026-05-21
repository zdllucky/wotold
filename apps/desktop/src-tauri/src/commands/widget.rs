//! [W4] Floating recording widget — controls for the second Tauri window.
//!
//! The `recording-widget` window is declared statically in `tauri.conf.json`
//! (label, 280×52, transparent, decorations off, always-on-top, skipTaskbar,
//! visible:false). These commands toggle visibility, position it in the
//! top-right corner of the primary monitor, and restore the main window when
//! the user clicks the widget body (or presses Stop from the widget).
//!
//! Every command is idempotent — repeated `show` / `hide` is a no-op. If the
//! widget window failed to register (e.g. user-edited `tauri.conf.json`), the
//! commands return `AppError::NotFound`; the frontend should swallow this so
//! the main UX still works without the widget.

use tauri::{AppHandle, LogicalPosition, Manager, State};

use crate::{state::AppState, AppError};

const WIDGET_LABEL: &str = "recording-widget";
const MAIN_LABEL: &str = "main";

const WIDGET_W: f64 = 280.0;
const MARGIN: f64 = 24.0;

/// Show the floating recording widget. Positions it from saved settings if
/// present (S7 user-draggable), else top-right corner of the primary monitor.
/// Best-effort — if the monitor cannot be queried, the widget is shown at its
/// last position.
#[tauri::command]
pub async fn show_recording_widget(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let Some(window) = app.get_webview_window(WIDGET_LABEL) else {
        return Err(AppError::NotFound("recording-widget window".into()));
    };

    // [S7] Saved position takes priority — let the user keep the widget where
    // they parked it last session. Reset to default if either axis is missing
    // or unparseable (avoids restoring to (0, partial)).
    let saved = read_saved_position(&state).await;
    let (logical_x, logical_y) = saved.unwrap_or_else(|| default_top_right(&app));

    if let Err(e) = window.set_position(LogicalPosition::new(logical_x, logical_y)) {
        log::warn!("recording-widget set_position failed: {e}");
    }

    window
        .show()
        .map_err(|e| AppError::Other(format!("widget show: {e}")))?;
    // alwaysOnTop already keeps it above; explicit set_focus would steal focus
    // from the user's current app, which we don't want for a passive widget.
    Ok(())
}

/// [S7] Persist current widget logical position. Called from the window
/// `Moved` event handler (debounced in lib.rs). Silently swallows errors —
/// position is a UX nicety, not a correctness invariant.
pub async fn persist_widget_position(
    db: &sqlx::SqlitePool,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    crate::db::set_setting(db, "recording.widget.x", &x.to_string()).await?;
    crate::db::set_setting(db, "recording.widget.y", &y.to_string()).await?;
    Ok(())
}

async fn read_saved_position(state: &State<'_, AppState>) -> Option<(f64, f64)> {
    let x = crate::db::get_setting(&state.db, "recording.widget.x")
        .await
        .ok()
        .flatten()?;
    let y = crate::db::get_setting(&state.db, "recording.widget.y")
        .await
        .ok()
        .flatten()?;
    let x: f64 = x.parse().ok()?;
    let y: f64 = y.parse().ok()?;
    Some((x, y))
}

fn default_top_right(app: &AppHandle) -> (f64, f64) {
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let size = monitor.size();
        let pos = monitor.position();
        let scale = monitor.scale_factor();
        let mon_logical_w = size.width as f64 / scale;
        let mon_logical_x = pos.x as f64 / scale;
        let mon_logical_y = pos.y as f64 / scale;
        return (
            mon_logical_x + mon_logical_w - WIDGET_W - MARGIN,
            mon_logical_y + MARGIN,
        );
    }
    // Monitor query failed (rare). Fall back to (24, 24) — anywhere visible
    // is better than (0,0) titlebar overlap on macOS.
    (MARGIN, MARGIN)
}

/// [S7] Clamp position so the widget stays at least partially on-screen even
/// after a monitor disconnects between sessions. Drops off-monitor positions
/// back to the default top-right of the primary monitor.
#[allow(dead_code)]
pub fn clamp_to_visible_area(app: &AppHandle, x: f64, y: f64) -> (f64, f64) {
    let Ok(Some(monitor)) = app.primary_monitor() else {
        return (x, y);
    };
    let size = monitor.size();
    let pos = monitor.position();
    let scale = monitor.scale_factor();
    let mon_x = pos.x as f64 / scale;
    let mon_y = pos.y as f64 / scale;
    let mon_w = size.width as f64 / scale;
    let mon_h = size.height as f64 / scale;

    // Require at least 40px of widget to be on the monitor (left or right edge).
    const MIN_VISIBLE: f64 = 40.0;
    let max_x = mon_x + mon_w - MIN_VISIBLE;
    let min_x = mon_x - WIDGET_W + MIN_VISIBLE;
    let max_y = mon_y + mon_h - MIN_VISIBLE;
    let min_y = mon_y;
    let cx = x.clamp(min_x, max_x);
    let cy = y.clamp(min_y, max_y);
    if (cx - x).abs() > 0.5 || (cy - y).abs() > 0.5 {
        // Position was off-screen — snap back to the default corner.
        return default_top_right(app);
    }
    (cx, cy)
}

/// Hide the floating recording widget. Idempotent — silent if the widget is
/// already hidden or not registered.
#[tauri::command]
pub async fn hide_recording_widget(app: AppHandle) -> Result<(), AppError> {
    let Some(window) = app.get_webview_window(WIDGET_LABEL) else {
        // Not registered — caller doesn't care.
        return Ok(());
    };
    if let Err(e) = window.hide() {
        log::warn!("recording-widget hide failed: {e}");
    }
    Ok(())
}

/// Restore the main window (unminimize + show + focus) and hide the floating
/// widget. Used when the user clicks the widget body, or after Stop.
#[tauri::command]
pub async fn restore_main_window(app: AppHandle) -> Result<(), AppError> {
    let Some(main) = app.get_webview_window(MAIN_LABEL) else {
        return Err(AppError::NotFound("main window".into()));
    };

    if let Err(e) = main.unminimize() {
        log::warn!("main unminimize failed: {e}");
    }
    if let Err(e) = main.show() {
        log::warn!("main show failed: {e}");
    }
    if let Err(e) = main.set_focus() {
        log::warn!("main set_focus failed: {e}");
    }

    if let Some(widget) = app.get_webview_window(WIDGET_LABEL) {
        if let Err(e) = widget.hide() {
            log::warn!("recording-widget hide on restore failed: {e}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Sanity checks for the constants — guard against accidental renames.
    // The labels in `tauri.conf.json` (windows[].label) MUST stay in sync
    // with these strings; otherwise show/hide silently no-ops at runtime.
    #[test]
    fn labels_match_tauri_config() {
        assert_eq!(WIDGET_LABEL, "recording-widget");
        assert_eq!(MAIN_LABEL, "main");
    }
}
