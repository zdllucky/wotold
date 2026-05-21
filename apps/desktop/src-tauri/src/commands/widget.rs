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

use tauri::{AppHandle, LogicalPosition, Manager};

use crate::AppError;

const WIDGET_LABEL: &str = "recording-widget";
const MAIN_LABEL: &str = "main";

/// Show the floating recording widget. Positions it in the top-right corner of
/// the primary monitor and brings it to the front. Best-effort — if the
/// monitor cannot be queried, the widget is shown at its last position.
#[tauri::command]
pub async fn show_recording_widget(app: AppHandle) -> Result<(), AppError> {
    let Some(window) = app.get_webview_window(WIDGET_LABEL) else {
        return Err(AppError::NotFound("recording-widget window".into()));
    };

    // Position in the top-right corner of the primary monitor BEFORE showing —
    // avoids a brief flash at (0,0) or the previous location.
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let size = monitor.size();
        let pos = monitor.position();
        let scale = monitor.scale_factor();

        // Logical coordinates so we don't have to multiply by scale ourselves.
        // The monitor reports physical pixels — convert to logical with scale.
        let mon_logical_w = size.width as f64 / scale;
        let mon_logical_x = pos.x as f64 / scale;
        let mon_logical_y = pos.y as f64 / scale;

        const WIDGET_W: f64 = 280.0;
        const MARGIN: f64 = 24.0;

        let logical_x = mon_logical_x + mon_logical_w - WIDGET_W - MARGIN;
        let logical_y = mon_logical_y + MARGIN;

        if let Err(e) = window.set_position(LogicalPosition::new(logical_x, logical_y)) {
            log::warn!("recording-widget set_position failed: {e}");
        }
    }

    window
        .show()
        .map_err(|e| AppError::Other(format!("widget show: {e}")))?;
    // alwaysOnTop already keeps it above; explicit set_focus would steal focus
    // from the user's current app, which we don't want for a passive widget.
    Ok(())
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
