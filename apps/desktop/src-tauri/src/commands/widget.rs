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

use std::time::Duration;

use tauri::{AppHandle, LogicalPosition, Manager, Monitor, State, WebviewWindow};

use crate::{state::AppState, AppError};

/// Snap animation tuning. ~200ms total over 12 frames at 60Hz.
const SNAP_FRAMES: u32 = 12;
const SNAP_FRAME_MS: u64 = 16;

const WIDGET_LABEL: &str = "recording-widget";
const MAIN_LABEL: &str = "main";

const WIDGET_W: f64 = 320.0;
const WIDGET_H: f64 = 84.0;
const MARGIN: f64 = 12.0;
// [S8] macOS menu bar высоко ~24px; берём 32 чтобы pill не лез под трекинг
// menu-bar regions. Bottom/left/right safe area = MARGIN.
const SAFE_AREA_TOP: f64 = 32.0;

/// Show the floating recording widget. Positions it from saved settings if
/// present (S7 user-draggable) AND saved position is on a currently-attached
/// monitor; else top-right corner of the monitor that contains the cursor.
/// This means swiping to another physical display gives the user the widget
/// on that display, not stuck on the previous monitor.
#[tauri::command]
pub async fn show_recording_widget(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let Some(window) = app.get_webview_window(WIDGET_LABEL) else {
        return Err(AppError::NotFound("recording-widget window".into()));
    };

    // [S8] Spaces / Mission Control: показать виджет на всех воркспейсах
    // текущего монитора. Tauri выставляет NSWindowCollectionBehavior
    // (canJoinAllSpaces + stationary). Idempotent — повторный вызов no-op.
    if let Err(e) = window.set_visible_on_all_workspaces(true) {
        log::warn!("set_visible_on_all_workspaces failed: {e}");
    }

    let saved = read_saved_position(&state).await;
    let cursor_monitor = current_cursor_monitor(&app);
    let target_pos = match (saved, &cursor_monitor) {
        // Saved position is on the same monitor where the cursor lives →
        // honour it (user drag persists across show/hide cycles).
        (Some((x, y)), Some(monitor)) if point_in_monitor(monitor, x, y) => (x, y),
        // Otherwise position top-right of whatever monitor the cursor is on.
        // [S8] Swipe to second display → widget follows the cursor.
        (_, Some(monitor)) => default_top_right_of(monitor),
        _ => default_top_right(&app),
    };

    if let Err(e) = window.set_position(LogicalPosition::new(target_pos.0, target_pos.1)) {
        log::warn!("recording-widget set_position failed: {e}");
    }

    window
        .show()
        .map_err(|e| AppError::Other(format!("widget show: {e}")))?;
    // alwaysOnTop already keeps it above; explicit set_focus would steal focus
    // from the user's current app, which we don't want for a passive widget.
    Ok(())
}

/// [S7/S8] Persist current widget logical position after clamping to safe
/// area. Called from the window `Moved` event handler (debounced in lib.rs).
/// Silently swallows errors — position is a UX nicety, not a correctness
/// invariant.
pub async fn persist_widget_position(
    app: &AppHandle,
    db: &sqlx::SqlitePool,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    let (sx, sy) = clamp_to_safe_area(app, x, y);
    crate::db::set_setting(db, "recording.widget.x", &sx.to_string()).await?;
    crate::db::set_setting(db, "recording.widget.y", &sy.to_string()).await?;
    Ok(())
}

/// [Widget v3] После того как drag settled, прибиваем widget к ближайшей
/// вертикальной стороне монитора (left/right edge) с MARGIN. Y сохраняется
/// где отпустил (clamp в safe area). Анимация — easeOutCubic по SNAP_FRAMES
/// кадрам, ~200ms total. Финальная позиция persist'ится в settings.
///
/// Вызывается из `lib.rs` после 400ms idle на `Moved` event. Caller обязан
/// выставить `is_animating=true` ДО вызова чтобы Moved events от наших
/// `set_position` не триггерили recursive snap.
pub async fn snap_to_nearest_side(
    app: AppHandle,
    widget: WebviewWindow,
    db: sqlx::SqlitePool,
    current_x: f64,
    current_y: f64,
) {
    let center_x = current_x + WIDGET_W / 2.0;
    let center_y = current_y + WIDGET_H / 2.0;

    let monitor = monitor_at_point(&app, center_x, center_y)
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };

    let scale = monitor.scale_factor();
    let mon_x = monitor.position().x as f64 / scale;
    let mon_y = monitor.position().y as f64 / scale;
    let mon_w = monitor.size().width as f64 / scale;
    let mon_h = monitor.size().height as f64 / scale;

    // LEFT vs RIGHT по центру widget'а относительно центра монитора.
    let target_x = if center_x < mon_x + mon_w / 2.0 {
        mon_x + MARGIN
    } else {
        mon_x + mon_w - WIDGET_W - MARGIN
    };

    let min_y = mon_y + SAFE_AREA_TOP;
    let max_y = mon_y + mon_h - WIDGET_H - MARGIN;
    let target_y = current_y.clamp(min_y, max_y);

    // easeOutCubic анимация: t' = 1 − (1 − t)^3.
    for i in 1..=SNAP_FRAMES {
        let t = i as f64 / SNAP_FRAMES as f64;
        let eased = 1.0 - (1.0 - t).powi(3);
        let x = current_x + (target_x - current_x) * eased;
        let y = current_y + (target_y - current_y) * eased;
        if let Err(e) = widget.set_position(LogicalPosition::new(x, y)) {
            log::warn!("snap step failed: {e}");
            break;
        }
        tokio::time::sleep(Duration::from_millis(SNAP_FRAME_MS)).await;
    }

    if let Err(e) = persist_widget_position(&app, &db, target_x, target_y).await {
        log::warn!("snap persist failed: {e}");
    }
}

/// Найти монитор содержащий точку (logical coords). Адаптация
/// `current_cursor_monitor` для произвольной точки, не только cursor.
fn monitor_at_point(app: &AppHandle, x: f64, y: f64) -> Option<Monitor> {
    let monitors = app.available_monitors().ok()?;
    monitors.into_iter().find(|m| point_in_monitor(m, x, y))
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
        return default_top_right_of(&monitor);
    }
    // Monitor query failed (rare). Fall back to (24, 24) — anywhere visible
    // is better than (0,0) titlebar overlap on macOS.
    (MARGIN, MARGIN)
}

fn default_top_right_of(monitor: &Monitor) -> (f64, f64) {
    let size = monitor.size();
    let pos = monitor.position();
    let scale = monitor.scale_factor();
    let mon_logical_w = size.width as f64 / scale;
    let mon_logical_x = pos.x as f64 / scale;
    let mon_logical_y = pos.y as f64 / scale;
    (
        mon_logical_x + mon_logical_w - WIDGET_W - MARGIN,
        mon_logical_y + SAFE_AREA_TOP,
    )
}

/// [S8] Найти монитор, на котором сейчас курсор. Tauri 2 даёт
/// `cursor_position()` (физические px) + `available_monitors()`.
fn current_cursor_monitor(app: &AppHandle) -> Option<Monitor> {
    let cursor = app.cursor_position().ok()?;
    let monitors = app.available_monitors().ok()?;
    monitors.into_iter().find(|m| {
        let pos = m.position();
        let size = m.size();
        let cx = cursor.x;
        let cy = cursor.y;
        cx >= pos.x as f64
            && cx < (pos.x + size.width as i32) as f64
            && cy >= pos.y as f64
            && cy < (pos.y + size.height as i32) as f64
    })
}

/// `(x, y)` в logical coords — лежит ли точка на этом мониторе.
fn point_in_monitor(monitor: &Monitor, x: f64, y: f64) -> bool {
    let pos = monitor.position();
    let size = monitor.size();
    let scale = monitor.scale_factor();
    let mon_x = pos.x as f64 / scale;
    let mon_y = pos.y as f64 / scale;
    let mon_w = size.width as f64 / scale;
    let mon_h = size.height as f64 / scale;
    x >= mon_x && x < mon_x + mon_w && y >= mon_y && y < mon_y + mon_h
}

/// [S8] Clamp logical position into safe area of whichever monitor contains
/// the widget. Enforces MARGIN от боковых/нижнего краёв и SAFE_AREA_TOP сверху
/// (под menu bar). Returns the clamped pair — used при persistence after drag
/// to keep widget away from screen edges.
pub fn clamp_to_safe_area(app: &AppHandle, x: f64, y: f64) -> (f64, f64) {
    let monitor = app
        .available_monitors()
        .ok()
        .and_then(|monitors| {
            monitors
                .into_iter()
                .find(|m| point_in_monitor(m, x + WIDGET_W / 2.0, y + WIDGET_H / 2.0))
        })
        .or_else(|| app.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        return (x, y);
    };
    let pos = monitor.position();
    let size = monitor.size();
    let scale = monitor.scale_factor();
    let mon_x = pos.x as f64 / scale;
    let mon_y = pos.y as f64 / scale;
    let mon_w = size.width as f64 / scale;
    let mon_h = size.height as f64 / scale;

    let min_x = mon_x + MARGIN;
    let max_x = mon_x + mon_w - WIDGET_W - MARGIN;
    let min_y = mon_y + SAFE_AREA_TOP;
    let max_y = mon_y + mon_h - WIDGET_H - MARGIN;
    (x.clamp(min_x, max_x), y.clamp(min_y, max_y))
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
