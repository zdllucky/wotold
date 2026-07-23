// [B27.6] Нативный share macOS: NSSharingServicePicker у прямоугольника
// кнопки. Не-macOS → честный Err (R4-стиль) — фронт фоллбечится копией.
//
// AppKit строго main-thread — вся работа внутри `run_on_main_thread`.
// Пикер обязан жить, пока открыт его поповер: держим Retained в thread_local
// (перезапись при следующем вызове), иначе немедленный drop закрывает меню.

use crate::error::AppError;

/// Конверсия CSS-координат вебвью (origin top-left) в AppKit-координаты
/// contentView (origin bottom-left). Логические px == пойнты на macOS.
#[cfg(any(target_os = "macos", test))]
fn anchor_rect(x: f64, y: f64, w: f64, h: f64, view_h: f64) -> (f64, f64, f64, f64) {
    (x, view_h - y - h, w, h)
}

#[tauri::command]
pub async fn share_text(
    window: tauri::WebviewWindow,
    text: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        window
            .clone()
            .run_on_main_thread(move || show_share_picker(&window, &text, x, y, w, h))
            .map_err(|e| AppError::Other(format!("share: main thread dispatch failed: {e}")))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, text, x, y, w, h);
        Err(AppError::Other("share: unsupported platform".into()))
    }
}

#[cfg(target_os = "macos")]
thread_local! {
    /// Удерживает последний пикер живым, пока открыт его поповер (main-thread).
    static LAST_PICKER: std::cell::RefCell<
        Option<objc2::rc::Retained<objc2_app_kit::NSSharingServicePicker>>,
    > = const { std::cell::RefCell::new(None) };
}

/// # Safety
///
/// `ns_window` указатель валиден от Tauri пока окно живёт (`window` держит его
/// на время вызова). Вызывается ТОЛЬКО из `run_on_main_thread` — AppKit
/// main-thread-only инвариант соблюдён (NSWindow/NSView в objc2-app-kit
/// типизированы MainThreadOnly; picker размечен AnyThread, но презентация —
/// AppKit UI и обязана идти на main, что и подтверждает MainThreadMarker).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn show_share_picker(window: &tauri::WebviewWindow, text: &str, x: f64, y: f64, w: f64, h: f64) {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::AnyThread;
    use objc2_app_kit::{NSSharingServicePicker, NSWindow};
    use objc2_foundation::{
        MainThreadMarker, NSArray, NSPoint, NSRect, NSRectEdge, NSSize, NSString,
    };

    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("share: not on main thread");
        return;
    };
    let ns_window_ptr = match window.ns_window() {
        Ok(ptr) if !ptr.is_null() => ptr as *mut NSWindow,
        Ok(_) => {
            log::warn!("share: ns_window is null");
            return;
        }
        Err(e) => {
            log::warn!("share: ns_window unavailable: {e}");
            return;
        }
    };
    // SAFETY: указатель валиден (см. док-коммент), мы на main thread.
    let ns_window: &NSWindow = unsafe { &*ns_window_ptr };
    let Some(content_view) = ns_window.contentView() else {
        log::warn!("share: window has no contentView");
        return;
    };

    let view_h = content_view.frame().size.height;
    let (ax, ay, aw, ah) = anchor_rect(x, y, w, h, view_h);
    let rect = NSRect::new(NSPoint::new(ax, ay), NSSize::new(aw, ah));

    let item: Retained<NSString> = NSString::from_str(text);
    let any: Retained<AnyObject> = Retained::into_super(Retained::into_super(item));
    let items: Retained<NSArray<AnyObject>> = NSArray::from_retained_slice(&[any]);

    // SAFETY: initWithItems/show* — задокументированные AppKit-инициализатор и
    // презентация; main thread гарантирован (mtm), items непустой NSArray.
    // mtm — только компайл/ран-тайм ассерт main-thread (AppKit-презентация);
    // сам класс в objc2-app-kit 0.3 размечен AnyThread.
    let _ = mtm;
    let picker =
        unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items) };
    picker.showRelativeToRect_ofView_preferredEdge(rect, &content_view, NSRectEdge::MinY);
    LAST_PICKER.with(|slot| *slot.borrow_mut() = Some(picker));
}

#[cfg(test)]
mod tests {
    use super::anchor_rect;

    #[test]
    fn anchor_rect_flips_to_bottom_left_origin() {
        // Кнопка 30×20 в точке (100, 50) при высоте вью 600:
        // AppKit y = 600 - 50 - 20 = 530.
        assert_eq!(
            anchor_rect(100.0, 50.0, 30.0, 20.0, 600.0),
            (100.0, 530.0, 30.0, 20.0)
        );
    }

    #[test]
    fn anchor_rect_bottom_edge_and_zero_size() {
        // Кнопка у нижнего края: y+h == view_h → AppKit y = 0.
        assert_eq!(
            anchor_rect(0.0, 580.0, 10.0, 20.0, 600.0),
            (0.0, 0.0, 10.0, 20.0)
        );
        // Нулевые размеры (jsdom/тесты) не ломают арифметику.
        assert_eq!(
            anchor_rect(0.0, 0.0, 0.0, 0.0, 600.0),
            (0.0, 600.0, 0.0, 0.0)
        );
    }
}
