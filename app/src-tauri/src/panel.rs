//! The panel window and the tray icon's highlight.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::model;

pub const PANEL: &str = "panel";
pub const TRAY: &str = "main";

#[derive(Default)]
pub struct Timing {
    /// When the panel was asked to open; cleared once its frame is painted.
    clicked: Option<Instant>,
    /// When the panel last hid, so the click that blurred it doesn't reopen it straight away.
    hidden: Option<Instant>,
    last_ms: Option<f64>,
}

/// Called by the page once the panel has painted after `panel-shown`; returns the measured time.
#[tauri::command]
pub fn panel_painted(timing: State<'_, Mutex<Timing>>) -> Option<f64> {
    let mut t = timing.lock().unwrap();
    if let Some(clicked) = t.clicked.take() {
        t.last_ms = Some(clicked.elapsed().as_secs_f64() * 1000.0);
    }
    t.last_ms
}

/// Keeps the menu bar icon highlighted while the panel is open, like a native menu bar extra.
/// tray-icon highlights it only for the duration of a mouse press, so this is re-applied after the
/// mouse-up and whenever the panel hides.
pub fn set_tray_highlight(app: &AppHandle, on: bool) {
    #[cfg(target_os = "macos")]
    if let Some(tray) = app.tray_by_id(TRAY) {
        let _ = tray.with_inner_tray_icon(move |inner| {
            let Some(mtm) = objc2::MainThreadMarker::new() else { return };
            if let Some(button) = inner.ns_status_item().and_then(|item| item.button(mtm)) {
                button.highlight(on);
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, on);
}

/// macOS: turns the panel into a non-activating panel (`NSPanel`) that can show on any Space, a
/// full-screen app's included, like Spotlight. macOS keeps an ordinary window, even one allowed on all
/// Spaces, off another app's full-screen Space, so clicking the icon there showed the panel out of sight.
pub fn make_panel(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Some(ptr) = app.get_webview_window(PANEL).and_then(|w| w.ns_window().ok()) {
        use objc2::ClassType;
        use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior as B, NSWindowStyleMask};
        // SAFETY: Tauri's NSWindow for the panel, which lives as long as the app; called on the main thread.
        // Its class becomes a subclass of NSPanel. Tao's own class only adds a `focusable` field, read by
        // the two methods ToolboxPanel replaces, and a drag-by-background `sendEvent:` the panel doesn't use.
        let window = unsafe {
            objc2::ffi::object_setClass(ptr.cast(), ToolboxPanel::class());
            &*ptr.cast::<NSWindow>()
        };
        window.setStyleMask(window.styleMask() | NSWindowStyleMask::NonactivatingPanel);
        window.setCollectionBehavior(B::CanJoinAllSpaces | B::FullScreenAuxiliary | B::IgnoresCycle);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[cfg(target_os = "macos")]
objc2::define_class!(
    /// The panel window's class: an NSPanel that takes keyboard focus (for the search field) without
    /// activating the app, so the Space doesn't switch.
    #[unsafe(super(objc2_app_kit::NSPanel, objc2_app_kit::NSWindow, objc2_app_kit::NSResponder, objc2_foundation::NSObject))]
    #[thread_kind = objc2::MainThreadOnly]
    #[name = "CodeToolboxPanel"]
    struct ToolboxPanel;

    impl ToolboxPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            false
        }
    }
);

pub fn panel_visible(app: &AppHandle) -> bool {
    app.get_webview_window(PANEL).is_some_and(|w| w.is_visible().unwrap_or(false))
}

pub fn hide_panel(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window(PANEL) {
        if panel.is_visible().unwrap_or(false) {
            app.state::<Mutex<Timing>>().lock().unwrap().hidden = Some(Instant::now());
        }
        let _ = panel.hide();
    }
    set_tray_highlight(app, false);
}

/// `from_tray`: place the panel under the icon. Otherwise (opened by a second launch) the icon's position
/// may not be known yet, so it goes to the top right, where the menu bar icons are.
pub fn toggle_panel(app: &AppHandle, from_tray: bool) {
    let Some(panel) = app.get_webview_window(PANEL) else { return };
    if panel.is_visible().unwrap_or(false) {
        hide_panel(app);
        return;
    }
    {
        let timing = app.state::<Mutex<Timing>>();
        let mut t = timing.lock().unwrap();
        // Clicking the icon while the panel is open first blurs (hides) it; that same click
        // must not open it again.
        if t.hidden.is_some_and(|h| h.elapsed() < Duration::from_millis(300)) {
            return;
        }
        t.clicked = Some(Instant::now());
    }
    let _ = panel.move_window(if from_tray { Position::TrayCenter } else { Position::TopRight });
    let _ = panel.show();
    // macOS: `show` already makes the panel the key window. Activating the app as well would take the user
    // off a full-screen app's Space, and the panel would hide as it lost focus.
    #[cfg(not(target_os = "macos"))]
    let _ = panel.set_focus();
    set_tray_highlight(app, true);
    let _ = panel.emit("panel-shown", ());
    model::panel_did_open(app);
}
