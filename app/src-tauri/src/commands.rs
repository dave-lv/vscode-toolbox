//! What the panel can ask for. Profiles are passed by id; the CLI name (`--profile`) is looked up
//! here, so the panel can never pass a name that would make the editor create a new profile.

use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;
use toolbox_core::uri::file_uri;
use toolbox_core::{LaunchRequest, bridge, scanner};

use crate::model::{self, show_banner, with_model};
use crate::panel;
use crate::ui::UiState;

fn cli_name(app: &AppHandle, profile_id: Option<&str>) -> Option<String> {
    let id = profile_id?;
    with_model(app, |m| m.profile(id).map(|p| p.cli_name.clone()))
}

#[tauri::command]
pub fn get_state(app: AppHandle) -> UiState {
    with_model(&app, |m| UiState::from_model(m, &app))
}

#[tauri::command]
pub fn open_project(app: AppHandle, uri: String, profile_id: Option<String>, new_window: bool, skip_layout: bool) {
    let Some(project) = with_model(&app, |m| m.project(&uri)) else {
        show_banner(&app, format!("Can't open {uri}"), true);
        return;
    };
    let paths = with_model(&app, |m| m.paths.clone());
    // An already-open project just gets focused (even with --new-window), so there's no layout to skip
    // and nothing would consume the marker.
    let mut marked = false;
    if skip_layout && !bridge::open_uris(&paths).contains(&uri) {
        match bridge::mark_skip_layout(&uri, &paths) {
            Ok(()) => marked = true,
            Err(e) => show_banner(&app, format!("Couldn't write the skip-layout marker: {e}"), true),
        }
    }
    let request = LaunchRequest::Project {
        uri: uri.clone(),
        is_workspace: project.is_workspace,
        profile_name: cli_name(&app, profile_id.as_deref()),
        new_window,
    };
    let on_failure: Option<Box<dyn FnOnce() + Send>> =
        marked.then(|| Box::new(move || bridge::clear_skip_layout(&uri, &paths)) as Box<dyn FnOnce() + Send>);
    model::launch(&app, request, on_failure);
}

#[tauri::command]
pub fn new_window(app: AppHandle, profile_id: String) {
    let Some(profile_name) = cli_name(&app, Some(&profile_id)) else { return };
    model::launch(&app, LaunchRequest::NewWindow { profile_name }, None);
}

#[tauri::command]
pub fn open_file(app: AppHandle, path: String, profile_id: Option<String>) {
    let profile_name = cli_name(&app, profile_id.as_deref());
    model::launch(&app, LaunchRequest::File { path, profile_name }, None);
}

#[tauri::command]
pub fn open_config(app: AppHandle) {
    let path = with_model(&app, |m| m.paths.config_file().to_string_lossy().into_owned());
    model::launch(&app, LaunchRequest::File { path, profile_name: None }, None);
}

/// Installs the dev-layout extension bundled with the app (its .vsix is a bundle resource) into a profile,
/// then removes the copy published as `mdri.dev-layout` before 0.3.0: both would build a layout.
#[tauri::command]
pub fn install_dev_layout(app: AppHandle, profile_id: String) {
    let Some(profile) = with_model(&app, |m| m.profile(&profile_id).cloned()) else { return };
    let vsix = match app.path().resolve(crate::ui::DEV_LAYOUT_VSIX, BaseDirectory::Resource) {
        Ok(p) if p.exists() => p.to_string_lossy().into_owned(),
        _ => {
            show_banner(&app, "The Dev Layout extension isn't included in this build", true);
            return;
        }
    };
    let profile_name = (!profile.is_default()).then(|| profile.cli_name.clone());
    let has_old = profile.extensions.iter().any(|e| e.id.eq_ignore_ascii_case(crate::ui::DEV_LAYOUT_OLD_ID));
    let installed = move |app: &AppHandle| {
        let text = format!("Installed Dev Layout in {}. Windows already open need a reload.", profile.name);
        show_banner(app, text, false);
        model::reload_editor(app); // the profile's extension list
    };
    let request = LaunchRequest::InstallExtension { vsix, profile_name: profile_name.clone() };
    model::run_cli(&app, request, move |app, result| {
        if result.is_err() {
            return;
        }
        if !has_old {
            installed(app);
            return;
        }
        let request = LaunchRequest::UninstallExtension { id: crate::ui::DEV_LAYOUT_OLD_ID.into(), profile_name };
        model::run_cli(app, request, move |app, result| {
            if result.is_ok() {
                installed(app);
            }
        });
    });
}

#[tauri::command]
pub fn close_window(app: AppHandle, uri: String) {
    let paths = with_model(&app, |m| m.paths.clone());
    if let Err(e) = bridge::request_close(&uri, &paths) {
        show_banner(&app, format!("Couldn't ask the editor to close the window: {e}"), true);
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        model::refresh_open(&app);
    });
}

#[tauri::command]
pub fn reveal(app: AppHandle, path: String) {
    panel::hide_panel(&app);
    if let Err(e) = app.opener().reveal_item_in_dir(&path) {
        show_banner(&app, format!("Couldn't show {path}: {e}"), true);
    }
}

#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) {
    match app.clipboard().write_text(text) {
        Ok(()) => show_banner(&app, "Copied", false),
        Err(e) => show_banner(&app, format!("Couldn't copy: {e}"), true),
    }
}

#[tauri::command]
pub fn toggle_favorite(app: AppHandle, uri: String) {
    model::update_config(&app, |c| {
        if let Some(i) = c.favorites.iter().position(|f| *f == uri) {
            c.favorites.remove(i);
        } else {
            c.favorites.push(uri.clone());
            c.hidden.retain(|h| *h != uri);
        }
    });
}

/// Drag-to-reorder: moves a favorite so it ends up at `to_index` (it's taken out, then inserted there).
#[tauri::command]
pub fn move_favorite(app: AppHandle, uri: String, to_index: usize) {
    model::update_config(&app, |c| {
        if let Some(i) = c.favorites.iter().position(|f| *f == uri) {
            let item = c.favorites.remove(i);
            c.favorites.insert(to_index.min(c.favorites.len()), item);
        }
    });
}

#[tauri::command]
pub fn hide(app: AppHandle, uri: String) {
    model::update_config(&app, |c| {
        c.favorites.retain(|f| *f != uri);
        if !c.hidden.contains(&uri) {
            c.hidden.push(uri.clone());
        }
    });
}

#[tauri::command]
pub fn unhide(app: AppHandle, uri: String) {
    model::update_config(&app, |c| c.hidden.retain(|h| *h != uri));
}

/// `name` None or empty removes the alias.
#[tauri::command]
pub fn rename(app: AppHandle, uri: String, name: Option<String>) {
    let name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    let base = with_model(&app, |m| m.project(&uri).map(|p| p.base_name));
    model::update_config(&app, |c| match &name {
        Some(n) if Some(n) != base.as_ref() => {
            c.aliases.insert(uri.clone(), n.clone());
        }
        _ => {
            c.aliases.remove(&uri);
        }
    });
}

/// The dev-layout agent for one project; None goes back to the profile's `devLayout.agent`.
#[tauri::command]
pub fn set_agent(app: AppHandle, uri: String, agent: Option<String>) {
    model::update_config(&app, |c| match agent.as_deref() {
        Some(a @ ("claude" | "codex")) => {
            c.agents.insert(uri.clone(), a.to_string());
        }
        _ => {
            c.agents.remove(&uri);
        }
    });
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    show_builtin_profiles: Option<bool>,
    show_git_status: Option<bool>,
    scan_enabled: Option<bool>,
    scan_roots: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    code_app: Option<String>,
}

#[tauri::command]
pub fn update_settings(app: AppHandle, patch: SettingsPatch) {
    model::update_config(&app, |c| {
        if let Some(v) = patch.show_builtin_profiles {
            c.show_builtin_profiles = v;
        }
        if let Some(v) = patch.show_git_status {
            c.show_git_status = v;
        }
        if let Some(v) = patch.scan_enabled {
            c.scan.enabled = v;
        }
        if let Some(v) = &patch.scan_roots {
            c.scan.roots = v.iter().map(|r| r.trim().to_string()).filter(|r| !r.is_empty()).collect();
        }
        if let Some(v) = &patch.exclude {
            c.exclude = v.iter().map(|r| r.trim().to_string()).filter(|r| !r.is_empty()).collect();
        }
        if let Some(v) = &patch.code_app {
            c.code_app = v.trim().to_string();
        }
    });
}

#[tauri::command]
pub fn reset_config(app: AppHandle) {
    model::reset_config(&app);
}

#[tauri::command]
pub fn import_toolbox_favorites(app: AppHandle) {
    let paths = with_model(&app, |m| m.paths.clone());
    let uris: Vec<String> = match scanner::toolbox_favorite_paths(&paths) {
        Ok(p) => p.iter().map(|p| file_uri(p, paths.os)).collect(),
        Err(e) => {
            show_banner(&app, format!("Couldn't read JetBrains Toolbox settings: {e}"), true);
            return;
        }
    };
    let mut added = 0;
    // Counted inside the change: update_config re-reads config.json first.
    let saved = model::update_config(&app, |c| {
        let new: Vec<String> = uris.iter().filter(|u| !c.favorites.contains(u)).cloned().collect();
        added = new.len();
        c.hidden.retain(|h| !new.contains(h));
        c.favorites.extend(new);
    });
    if saved {
        let text = match added {
            0 => "No new favorites in JetBrains Toolbox".to_string(),
            n => format!("Imported {n} favorite{} from JetBrains Toolbox", if n == 1 { "" } else { "s" }),
        };
        show_banner(&app, text, false);
    }
}

#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, on: bool) {
    let result = if on { app.autolaunch().enable() } else { app.autolaunch().disable() };
    if let Err(e) = result {
        show_banner(&app, format!("Launch at login: {e}"), true);
    }
    model::emit_state(&app);
}

#[tauri::command]
pub fn dismiss_banner(app: AppHandle) {
    with_model(&app, |m| m.banner = None);
    model::emit_state(&app);
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) {
    panel::hide_panel(&app);
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}
