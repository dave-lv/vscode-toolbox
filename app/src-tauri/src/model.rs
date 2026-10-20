//! All app state, kept in Rust; the panel only renders `UiState` and sends commands.
//! Background work (reading the editor's state, git, the folder scan, launching) runs on threads and
//! applies its result only if no newer run of the same kind started meanwhile (generation counters).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use toolbox_core::platform::{EDITORS, Editor, Os, Paths, VSCODE};
use toolbox_core::{
    Catalog, Config, ConfigStore, GitInfo, Installation, LaunchRequest, LoadResult, Profile, Project, Snapshot,
    VsCodeReader, bridge, git, launcher, scanner,
};

const GIT_THROTTLE: Duration = Duration::from_secs(30);
const SCAN_THROTTLE: Duration = Duration::from_secs(60);
const GIT_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Banner {
    pub id: u64,
    pub text: String,
    pub is_error: bool,
}

pub struct Model {
    pub paths: Paths,
    pub config: Config,
    /// Set while config.json exists but can't be read or parsed; nothing is saved until it's fixed or reset.
    pub config_error: Option<String>,
    pub installation: Option<Installation>,
    pub snapshot: Snapshot,
    pub catalog: Catalog,
    pub open: HashSet<String>,
    pub git: HashMap<String, GitInfo>,
    pub banner: Option<Banner>,
    shell_env: Option<HashMap<String, String>>,
    scanned: Vec<String>,
    git_fetched_at: Option<Instant>,
    /// Folders the last git refresh covered; a catalog with others triggers a new one.
    git_targets: HashSet<String>,
    scanned_at: Option<Instant>,
    read_gen: u64,
    git_gen: u64,
    scan_gen: u64,
    banner_seq: u64,
    system_locale: String,
}

impl Model {
    pub fn new() -> Model {
        // config.json's location doesn't depend on the editor, so read it first: on macOS its `codeApp`
        // can point at the VS Code to use.
        let config = match ConfigStore::load(&Paths::current(&VSCODE)) {
            LoadResult::Loaded(c) => c,
            _ => Config::default(),
        };
        let (editor, installation) = detect_editor(&config);
        Model {
            paths: Paths::current(editor),
            config,
            config_error: None,
            installation,
            snapshot: Snapshot::default(),
            catalog: Catalog::default(),
            open: HashSet::new(),
            git: HashMap::new(),
            banner: None,
            shell_env: None,
            scanned: vec![],
            git_fetched_at: None,
            git_targets: HashSet::new(),
            scanned_at: None,
            read_gen: 0,
            git_gen: 0,
            scan_gen: 0,
            banner_seq: 0,
            system_locale: sys_locale::get_locale().unwrap_or_else(|| "en".into()),
        }
    }

    fn git_targets_now(&self) -> HashSet<String> {
        self.catalog.all().filter(|p| p.exists()).filter_map(Project::git_path).collect()
    }

    /// Rebuilds the catalog. Returns true when git status should refresh now: the catalog has folders
    /// the last refresh didn't cover (new projects from the editor or the scan).
    fn rebuild_catalog(&mut self) -> bool {
        let scanned = if self.config.scan.enabled { self.scanned.as_slice() } else { &[] };
        self.catalog = Catalog::build(&self.snapshot.projects, scanned, &self.config, &self.paths);
        self.config.show_git_status && !self.git_targets_now().is_subset(&self.git_targets)
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.snapshot.profiles.iter().find(|p| p.id == id)
    }

    /// A project from the lists, or built from its URI (e.g. a favorite the editor no longer lists).
    pub fn project(&self, uri: &str) -> Option<Project> {
        self.catalog
            .all()
            .chain(&self.catalog.hidden)
            .chain(&self.snapshot.projects)
            .find(|p| p.uri == uri)
            .cloned()
            .or_else(|| Project::make(uri, &self.paths, None))
    }
}

/// The first editor that's installed, in table order (VS Code first); VS Code if none is.
fn detect_editor(config: &Config) -> (&'static Editor, Option<Installation>) {
    for editor in EDITORS {
        let paths = Paths::current(editor);
        let app_override = (paths.os == Os::Macos && editor.id == VSCODE.id).then_some(config.code_app.as_str());
        if let Some(inst) = Installation::detect(&paths, app_override) {
            return (editor, Some(inst));
        }
    }
    (&VSCODE, None)
}

// MARK: - Access and events

pub fn with_model<T>(app: &AppHandle, f: impl FnOnce(&mut Model) -> T) -> T {
    let state = app.state::<Mutex<Model>>();
    let mut model = state.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut model)
}

pub fn emit_state(app: &AppHandle) {
    let state = with_model(app, |m| crate::ui::UiState::from_model(m, app));
    let _ = app.emit("state-changed", state);
}

pub fn show_banner(app: &AppHandle, text: impl Into<String>, is_error: bool) {
    let id = with_model(app, |m| {
        m.banner_seq += 1;
        m.banner = Some(Banner { id: m.banner_seq, text: text.into(), is_error });
        m.banner_seq
    });
    emit_state(app);
    if !is_error {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(4));
            if with_model(&app, |m| {
                let same = m.banner.as_ref().is_some_and(|b| b.id == id);
                if same {
                    m.banner = None;
                }
                same
            }) {
                emit_state(&app);
            }
        });
    }
}

// MARK: - Refresh

/// The panel was shown: re-read everything, including git and the folder scan (both throttled).
pub fn panel_did_open(app: &AppHandle) {
    load_config(app);
    reload_editor(app);
    refresh_open(app);
    refresh_git(app, false);
    rescan(app, false);
}

pub fn reload_editor(app: &AppHandle) {
    let (paths, installation, locale, generation) = with_model(app, |m| {
        m.read_gen += 1;
        (m.paths.clone(), m.installation.clone(), m.system_locale.clone(), m.read_gen)
    });
    let app = app.clone();
    std::thread::spawn(move || {
        let snapshot =
            VsCodeReader { paths: &paths, installation: installation.as_ref(), system_locale: locale }.read();
        let (changed, git_needed) = with_model(&app, |m| {
            if generation != m.read_gen || snapshot == m.snapshot {
                return (false, false);
            }
            m.snapshot = snapshot;
            (true, m.rebuild_catalog())
        });
        if git_needed {
            refresh_git(&app, true);
        }
        if changed {
            emit_state(&app);
        }
    });
}

pub fn refresh_open(app: &AppHandle) {
    let paths = with_model(app, |m| m.paths.clone());
    let open = bridge::open_uris(&paths);
    if with_model(app, |m| {
        let changed = m.open != open;
        m.open = open;
        changed
    }) {
        emit_state(app);
    }
}

pub fn refresh_git(app: &AppHandle, force: bool) {
    let job = with_model(app, |m| {
        if !m.config.show_git_status {
            m.git_gen += 1;
            m.git_targets.clear();
            let had = !m.git.is_empty();
            m.git.clear();
            return Err(had);
        }
        if !force && m.git_fetched_at.is_some_and(|t| t.elapsed() < GIT_THROTTLE) {
            return Err(false);
        }
        m.git_fetched_at = Some(Instant::now());
        m.git_targets = m.git_targets_now();
        m.git_gen += 1;
        Ok((m.git_targets.iter().cloned().collect::<Vec<_>>(), m.git_gen))
    });
    let (targets, generation) = match job {
        Ok(job) => job,
        Err(cleared) => {
            if cleared {
                emit_state(app);
            }
            return;
        }
    };
    let app = app.clone();
    std::thread::spawn(move || {
        // A few repos at a time: each is its own `git` process.
        let mut result = HashMap::new();
        for chunk in targets.chunks(6) {
            let found: Vec<(String, Option<GitInfo>)> = std::thread::scope(|s| {
                let handles: Vec<_> =
                    chunk.iter().map(|p| s.spawn(move || (p.clone(), git::read(p, GIT_TIMEOUT)))).collect();
                handles.into_iter().filter_map(|h| h.join().ok()).collect()
            });
            result.extend(found.into_iter().filter_map(|(p, g)| g.map(|g| (p, g))));
        }
        if with_model(&app, |m| {
            let current = generation == m.git_gen && m.git != result;
            if current {
                m.git = result;
            }
            current
        }) {
            emit_state(&app);
        }
    });
}

pub fn rescan(app: &AppHandle, force: bool) {
    let job = with_model(app, |m| {
        if !m.config.scan.enabled || (!force && m.scanned_at.is_some_and(|t| t.elapsed() < SCAN_THROTTLE)) {
            return None;
        }
        m.scanned_at = Some(Instant::now());
        m.scan_gen += 1;
        Some((m.config.scan.clone(), m.paths.clone(), m.scan_gen))
    });
    let Some((scan, paths, generation)) = job else { return };
    let app = app.clone();
    std::thread::spawn(move || {
        let found = scanner::scan(&scan.roots, scan.depth, &paths);
        // A newer scan (or a settings change) replaced this one.
        let applied = with_model(&app, |m| {
            if generation != m.scan_gen || scan != m.config.scan {
                return None;
            }
            m.scanned = found;
            Some(m.rebuild_catalog())
        });
        if let Some(git_needed) = applied {
            if git_needed {
                refresh_git(&app, true);
            }
            emit_state(&app);
        }
    });
}

// MARK: - Config

/// Reads config.json into the model. Returns the bytes the model's config was parsed from (None if the
/// file is missing or broken), the base `update_config` saves against: they always match the model.
pub fn load_config(app: &AppHandle) -> Option<Vec<u8>> {
    crate::watcher::follow_config_link(app);
    let paths = with_model(app, |m| m.paths.clone());
    for attempt in 0..2 {
        let (result, bytes) = ConfigStore::load_with_bytes(&paths);
        match result {
            LoadResult::Loaded(c) => {
                with_model(app, |m| m.config_error = None);
                apply_config(app, c);
                return bytes;
            }
            LoadResult::Missing => {
                let config = with_model(app, |m| {
                    m.config_error = None;
                    m.config.clone()
                });
                if attempt > 0 {
                    return None; // couldn't create it; still missing
                }
                // Create it so it can be edited by hand, then read it back like any other file: if
                // something else created it first, the model takes that file, not this one.
                let _ = ConfigStore::save_if_unchanged(&config, &paths, None);
            }
            LoadResult::Invalid(message) => {
                let new = with_model(app, |m| {
                    let new = m.config_error.as_deref() != Some(&message);
                    m.config_error = Some(message.clone());
                    new
                });
                if new {
                    show_banner(
                        app,
                        format!("{message} Changes aren't saved until it's fixed (or reset in Settings)."),
                        true,
                    );
                }
                return None;
            }
        }
    }
    None
}

/// Makes `c` the current config and starts whatever the change needs, whether it came from Settings or
/// from editing config.json by hand.
fn apply_config(app: &AppHandle, c: Config) {
    let changes = with_model(app, |m| {
        if c == m.config {
            return None;
        }
        let old = std::mem::replace(&mut m.config, c);
        let scan_changed = m.config.scan != old.scan;
        if scan_changed {
            // Results for the old roots no longer apply; drop them and any scan still running.
            m.scan_gen += 1;
            m.scanned.clear();
        }
        let app_changed = m.config.code_app != old.code_app;
        if app_changed {
            let (editor, installation) = detect_editor(&m.config);
            m.paths = Paths::current(editor);
            m.installation = installation;
        }
        let git_needed = m.rebuild_catalog(); // turning git status on refreshes it here
        let git_off = old.show_git_status && !m.config.show_git_status;
        Some((git_needed || git_off, scan_changed, app_changed))
    });
    let Some((git, scan, editor)) = changes else { return };
    if git {
        refresh_git(app, true);
    }
    if scan {
        rescan(app, true);
    }
    if editor {
        reload_editor(app);
    }
    emit_state(app);
}

/// Changes config.json: reads it, applies `change`, and saves only if the file is still what was read;
/// if it was edited meanwhile (by hand, say), it reads again and redoes the change. The model takes the
/// new config only once it's saved. Nothing is saved while the file is broken. Returns false if the
/// change wasn't saved (the reason is shown).
pub fn update_config(app: &AppHandle, mut change: impl FnMut(&mut Config)) -> bool {
    for _ in 0..3 {
        let base = load_config(app);
        let (error, mut c, paths) = with_model(app, |m| (m.config_error.clone(), m.config.clone(), m.paths.clone()));
        if let Some(error) = error {
            show_banner(app, format!("{error} Fix it, or reset it in Settings, before changing settings."), true);
            return false;
        }
        let before = c.clone();
        change(&mut c);
        if c == before {
            return true;
        }
        match ConfigStore::save_if_unchanged(&c, &paths, base.as_deref()) {
            Ok(true) => {
                apply_config(app, c);
                return true;
            }
            Ok(false) => continue, // edited on disk meanwhile: read it again
            Err(e) => {
                show_banner(app, format!("Couldn't save config.json: {e}"), true);
                return false;
            }
        }
    }
    show_banner(app, "config.json kept changing while saving; try again", true);
    false
}

pub fn reset_config(app: &AppHandle) {
    // It may have been fixed (e.g. via Edit) since the error was shown; then there's nothing to reset.
    load_config(app);
    let (error, config, paths) = with_model(app, |m| (m.config_error.clone(), m.config.clone(), m.paths.clone()));
    if error.is_none() {
        show_banner(app, "config.json is valid now; nothing was reset", false);
        return;
    }
    match ConfigStore::replace_broken(&config, &paths) {
        Ok(backup) => {
            with_model(app, |m| m.config_error = None);
            let note =
                backup.map(|b| format!("; the old one is {}", paths.tilde(&b.to_string_lossy()))).unwrap_or_default();
            show_banner(app, format!("Saved a new config.json{note}"), false);
        }
        Err(e) => show_banner(app, format!("Couldn't reset config.json: {e}"), true),
    }
}

// MARK: - Launching

/// Starts the editor's CLI with the login-shell environment (resolved once, in the background). The
/// panel hides first. `on_failure` runs if the editor couldn't be started or exited with an error.
pub fn launch(app: &AppHandle, request: LaunchRequest, on_failure: Option<Box<dyn FnOnce() + Send>>) {
    crate::panel::hide_panel(app);
    run_cli(app, request, move |_, result| {
        if let (Err(_), Some(f)) = (result, on_failure) {
            f();
        }
    });
}

/// Runs the editor's CLI on a background thread. `on_done` gets the outcome once: the CLI's exit, or
/// whatever kept it from starting (no installation, a spawn error). Errors also go to the banner.
pub fn run_cli(
    app: &AppHandle,
    request: LaunchRequest,
    on_done: impl FnOnce(&AppHandle, Result<(), String>) + Send + 'static,
) {
    let on_done = std::sync::Arc::new(Mutex::new(Some(on_done)));
    let finish = move |app: &AppHandle, result: Result<(), String>| {
        if let Err(error) = &result {
            show_banner(app, error.clone(), true);
        }
        if let Some(f) = on_done.lock().unwrap_or_else(|e| e.into_inner()).take() {
            f(app, result);
        }
    };
    let Some((installation, env, home)) =
        with_model(app, |m| Some((m.installation.clone()?, m.shell_env.clone(), PathBuf::from(m.paths.home()))))
    else {
        let name = with_model(app, |m| m.paths.editor.name);
        finish(app, Err(format!("{name} wasn't found. On macOS, check the app path in Settings.")));
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        let env = env.or_else(|| {
            let env = resolve_shell_env();
            with_model(&app, |m| m.shell_env.clone_from(&env));
            env
        });
        let (exit_app, exit_finish) = (app.clone(), finish.clone());
        let result = launcher::run(&installation, &request, env.as_ref(), &home, move |error| {
            exit_finish(&exit_app, error.map_or(Ok(()), Err));
        });
        if let Err(error) = result {
            finish(&app, Err(error));
        }
    });
}

/// The login shell's environment (macOS and Linux); the editor started from its CLI doesn't resolve it.
pub fn resolve_shell_env() -> Option<HashMap<String, String>> {
    #[cfg(unix)]
    {
        toolbox_core::shell_env::resolve(None, Duration::from_secs(10))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Resolves the shell environment ahead of the first launch.
pub fn warm_up(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let env = resolve_shell_env();
        with_model(&app, |m| {
            if m.shell_env.is_none() {
                m.shell_env = env;
            }
        });
    });
}
