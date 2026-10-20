//! What the panel renders: one serializable snapshot of everything, sent on every change
//! (`state-changed`) and on request (`get_state`). A few KB, so no diffs.

use std::sync::LazyLock;
use std::time::UNIX_EPOCH;

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use toolbox_core::platform::Os;
use toolbox_core::{ExtensionInfo, GitInfo, Project, ProjectKind};

use crate::model::{Banner, Model};

/// The dev-layout extension's .vsix in the app's resources (bundle.resources in tauri.conf.json).
pub const DEV_LAYOUT_VSIX: &str = "dev-layout.vsix";
/// The extension's id before 0.3.0 (publisher `mdri`); installing replaces it.
pub const DEV_LAYOUT_OLD_ID: &str = "mdri.dev-layout";

/// The bundled extension's version, so the panel can tell an older installed copy.
static DEV_LAYOUT_VERSION: LazyLock<String> = LazyLock::new(|| {
    let manifest = include_str!("../../../vscode-dev-layout/package.json");
    serde_json::from_str::<serde_json::Value>(manifest)
        .ok()
        .and_then(|v| v["version"].as_str().map(str::to_string))
        .unwrap_or_default()
});

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub os: Os,
    pub editor_name: String,
    pub version: Option<String>,
    /// The editor's installation was found (launching works).
    pub installed: bool,
    pub favorites: Vec<Row>,
    pub recent: Vec<Row>,
    /// Only filled when the folder scan is on.
    pub other: Vec<Row>,
    pub hidden: Vec<Row>,
    /// All profiles, built-in ones included (rows show their badges); the Profiles tab filters.
    pub profiles: Vec<ProfileInfo>,
    pub settings: Settings,
    pub config_error: Option<String>,
    pub config_path: String,
    pub banner: Option<Banner>,
    pub launch_at_login: bool,
    pub warnings: Vec<String>,
    pub dev_layout_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub uri: String,
    /// Alias from config.json, else the project's own name.
    pub name: String,
    pub base_name: String,
    pub detail: String,
    pub tag: Option<String>,
    pub kind: ProjectKind,
    pub is_workspace: bool,
    /// ms since the epoch.
    pub last_opened: Option<u64>,
    pub profile_id: Option<String>,
    pub host_path: Option<String>,
    /// What "Copy Path" copies: the local folder, or the decoded path on the remote (`/app`).
    pub path: String,
    pub open: bool,
    pub favorite: bool,
    pub git: Option<GitInfo>,
    /// The dev-layout agent chosen for this project; None: the profile's setting.
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub is_builtin: bool,
    pub is_default: bool,
    pub uses_default_extensions: bool,
    pub extensions: Vec<ExtensionInfo>,
    /// Favorites and recent projects associated with this profile.
    pub project_count: usize,
    pub settings_file: String,
    pub keybindings_file: String,
    pub directory: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub show_builtin_profiles: bool,
    pub show_git_status: bool,
    pub scan_enabled: bool,
    pub scan_roots: Vec<String>,
    pub scan_depth: usize,
    pub exclude: Vec<String>,
    /// macOS only.
    pub code_app: String,
}

impl UiState {
    pub fn from_model(m: &Model, app: &AppHandle) -> UiState {
        let row = |p: &Project| Row {
            uri: p.uri.clone(),
            name: m.config.aliases.get(&p.uri).cloned().unwrap_or_else(|| p.base_name.clone()),
            base_name: p.base_name.clone(),
            detail: p.detail.clone(),
            tag: p.tag.clone(),
            kind: p.kind,
            is_workspace: p.is_workspace,
            last_opened: p.last_opened.and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64),
            profile_id: p.profile_id.clone(),
            host_path: p.host_path.clone(),
            path: p
                .host_path
                .clone()
                .or_else(|| toolbox_core::VsUri::parse(&p.uri).map(|u| u.path))
                .unwrap_or_else(|| p.uri.clone()),
            open: m.open.contains(&p.uri),
            favorite: m.config.favorites.contains(&p.uri),
            git: if m.config.show_git_status { p.git_path().and_then(|g| m.git.get(&g).cloned()) } else { None },
            agent: m.config.agents.get(&p.uri).cloned(),
        };
        let rows = |list: &[Project]| list.iter().map(row).collect::<Vec<_>>();
        let listed = || m.catalog.favorites.iter().chain(&m.catalog.recent);

        UiState {
            os: m.paths.os,
            editor_name: m.paths.editor.name.to_string(),
            version: m.snapshot.version.clone(),
            installed: m.installation.is_some(),
            favorites: rows(&m.catalog.favorites),
            recent: rows(&m.catalog.recent),
            other: if m.config.scan.enabled { rows(&m.catalog.other) } else { vec![] },
            hidden: rows(&m.catalog.hidden),
            profiles: m
                .snapshot
                .profiles
                .iter()
                .map(|p| ProfileInfo {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    icon: p.icon.clone(),
                    is_builtin: p.is_builtin,
                    is_default: p.is_default(),
                    uses_default_extensions: p.uses_default_extensions,
                    extensions: p.extensions.clone(),
                    project_count: listed().filter(|x| x.profile_id.as_deref() == Some(p.id.as_str())).count(),
                    settings_file: p.settings_file.to_string_lossy().into_owned(),
                    keybindings_file: p.keybindings_file.to_string_lossy().into_owned(),
                    directory: p.directory.to_string_lossy().into_owned(),
                })
                .collect(),
            settings: Settings {
                show_builtin_profiles: m.config.show_builtin_profiles,
                show_git_status: m.config.show_git_status,
                scan_enabled: m.config.scan.enabled,
                scan_roots: m.config.scan.roots.clone(),
                scan_depth: m.config.scan.depth,
                exclude: m.config.exclude.clone(),
                code_app: m.config.code_app.clone(),
            },
            config_error: m.config_error.clone(),
            config_path: m.paths.tilde(&m.paths.config_file().to_string_lossy()),
            banner: m.banner.clone(),
            launch_at_login: app.autolaunch().is_enabled().unwrap_or(false),
            warnings: m.snapshot.warnings.clone(),
            dev_layout_version: DEV_LAYOUT_VERSION.clone(),
        }
    }
}
