// Code Toolbox: a tray icon that opens a small panel listing the editor's projects and profiles.
// All state and work live in Rust (model.rs on top of toolbox-core); the React panel renders `UiState`
// and calls the commands in commands.rs. See PLAN-cross-platform.md.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod model;
mod panel;
mod ui;
mod updates;
mod watcher;

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};

use crate::panel::{PANEL, TRAY, Timing};

fn main() {
    tauri::Builder::default()
        // Must be the first plugin. Launching the app again doesn't start a second copy; with
        // `--toggle-panel` it opens or closes the panel (also how the panel is reached where there is
        // no clickable tray icon, e.g. Linux AppIndicator).
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == "--toggle-panel") {
                panel::toggle_panel(app, false);
            }
        }))
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_autostart::Builder::new().app_name("Code Toolbox").build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::Pending::default())
        .manage(Mutex::new(Timing::default()))
        .manage(Mutex::new(model::Model::new()))
        .manage(watcher::WatcherState::default())
        .invoke_handler(tauri::generate_handler![
            panel::panel_painted,
            commands::get_state,
            commands::open_project,
            commands::new_window,
            commands::open_file,
            commands::open_config,
            commands::install_dev_layout,
            commands::close_window,
            commands::reveal,
            commands::copy_text,
            commands::toggle_favorite,
            commands::move_favorite,
            commands::hide,
            commands::unhide,
            commands::rename,
            commands::set_agent,
            commands::update_settings,
            commands::reset_config,
            commands::check_updates,
            commands::install_update,
            commands::dismiss_update,
            commands::import_toolbox_favorites,
            commands::set_launch_at_login,
            commands::dismiss_banner,
            commands::hide_panel,
            commands::quit,
        ])
        .setup(|app| {
            // Menu bar only: no Dock icon, no app menu.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle();
            panel::make_panel(handle);
            watcher::start(handle);
            model::load_config(handle);
            model::reload_editor(handle);
            model::refresh_open(handle);
            model::warm_up(handle);
            updates::start(handle);

            let quit = MenuItem::with_id(app, "quit", "Quit Code Toolbox", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit])?;
            // macOS tints a black template icon to fit the menu bar; Windows and Linux trays don't, and
            // black would vanish on a dark taskbar, so they get the coloured app icon.
            #[cfg(target_os = "macos")]
            let icon = include_bytes!("../icons/tray.png");
            #[cfg(not(target_os = "macos"))]
            let icon = include_bytes!("../icons/32x32.png");
            TrayIconBuilder::with_id(TRAY)
                .icon(Image::from_bytes(icon)?)
                .icon_as_template(cfg!(target_os = "macos"))
                .tooltip("Code Toolbox")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    // Lets the positioner know where the icon is.
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state, .. } = event {
                        match button_state {
                            // Open/close on press, like native menu bar extras.
                            MouseButtonState::Down => panel::toggle_panel(tray.app_handle(), true),
                            // tray-icon has just cleared the highlight; restore it if the panel is open.
                            MouseButtonState::Up => {
                                panel::set_tray_highlight(tray.app_handle(), panel::panel_visible(tray.app_handle()))
                            }
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == PANEL
                && let WindowEvent::Focused(false) = event
            {
                panel::hide_panel(window.app_handle());
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Code Toolbox");
}
