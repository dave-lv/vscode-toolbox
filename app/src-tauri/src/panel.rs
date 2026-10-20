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
    let _ = panel.set_focus();
    set_tray_highlight(app, true);
    let _ = panel.emit("panel-shown", ());
    model::panel_did_open(app);
}
