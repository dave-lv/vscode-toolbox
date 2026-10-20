//! Watches the editor's state, config.json (and its symlink target) and the open-window markers, so the
//! panel is current before it opens. Events are batched for 300 ms; the editor rewrites `state.vscdb`
//! files constantly, so only the files that change what's shown count.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Manager};
use toolbox_core::ConfigStore;

use crate::model::{self, with_model};

/// The running watcher and the config file it follows (config.json's symlink target, resolved).
#[derive(Default)]
pub struct WatcherState(Mutex<Option<(RecommendedWatcher, PathBuf)>>);

fn config_target(app: &AppHandle) -> PathBuf {
    let paths = with_model(app, |m| m.paths.clone());
    let target = ConfigStore::target(&paths);
    // Events carry real paths (e.g. /private/tmp), so resolve the folder; the file may not exist yet.
    let dir = target.parent().and_then(|d| std::fs::canonicalize(d).ok()).unwrap_or_else(|| paths.config_dir());
    dir.join(target.file_name().unwrap_or_default())
}

pub fn start(app: &AppHandle) {
    let paths = with_model(app, |m| m.paths.clone());
    let target = config_target(app);
    let _ = std::fs::create_dir_all(paths.open_windows_dir());

    let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
    let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            let _ = tx.send(event.paths);
        }
    });
    let Ok(mut watcher) = watcher else { return };
    let storage_dir = paths.storage_json().parent().map(Path::to_path_buf);
    let mut dirs: Vec<(PathBuf, RecursiveMode)> =
        vec![(paths.workspace_storage(), RecursiveMode::Recursive), (paths.config_dir(), RecursiveMode::Recursive)];
    dirs.extend(storage_dir.map(|d| (d, RecursiveMode::NonRecursive)));
    if let Some(dir) = target.parent().filter(|d| !d.starts_with(paths.config_dir())) {
        dirs.push((dir.to_path_buf(), RecursiveMode::NonRecursive));
    }
    for (dir, mode) in dirs {
        let _ = watcher.watch(&dir, mode); // a folder the editor hasn't created yet is fine
    }

    let app2 = app.clone();
    let target2 = target.clone();
    std::thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let mut changed = first;
            while let Ok(more) = rx.recv_timeout(Duration::from_millis(300)) {
                changed.extend(more);
            }
            dispatch(&app2, &changed, &target2);
        }
    });

    *app.state::<WatcherState>().0.lock().unwrap() = Some((watcher, target));
}

fn dispatch(app: &AppHandle, changed: &[PathBuf], config_target: &Path) {
    let paths = with_model(app, |m| m.paths.clone());
    let (config_dir, open_dir) = (paths.config_dir(), paths.open_windows_dir());
    let name = |p: &PathBuf| p.file_name().and_then(|n| n.to_str()).map(str::to_string).unwrap_or_default();
    let editor = changed.iter().any(|p| matches!(name(p).as_str(), "storage.json" | "workspace.json"));
    let open = changed.iter().any(|p| p.starts_with(&open_dir));
    let config = changed.iter().any(|p| p == config_target || (p.starts_with(&config_dir) && name(p) == "config.json"));

    if config {
        model::load_config(app);
    }
    if editor {
        model::reload_editor(app);
    }
    if open {
        model::refresh_open(app);
    }
}

/// Re-points the watcher when config.json starts linking somewhere else.
pub fn follow_config_link(app: &AppHandle) {
    let target = config_target(app);
    let current = app.state::<WatcherState>().0.lock().unwrap().as_ref().map(|(_, t)| t.clone());
    if current.is_some_and(|t| t != target) {
        start(app);
    }
}
