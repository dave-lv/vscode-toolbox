use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Serialize, Serializer};

use crate::platform::Paths;
use crate::uri::{RemoteTarget, VsUri};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectKind {
    Folder,
    WorkspaceFile,
    AttachedContainer,
    DevContainer,
    Ssh,
    Wsl,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// The URI exactly as the editor stores it; also the key in config.json.
    pub uri: String,
    pub kind: ProjectKind,
    /// Default display name (config aliases override it in the UI).
    pub base_name: String,
    /// Second line: `~/Documents/Projects/acme/shop`, `container shop-shop.dev-1 · /app`, …
    pub detail: String,
    /// Folder on this machine, if there is one (file manager, git status, existence check).
    pub host_path: Option<String>,
    /// Small tag next to the name: "container", "dev container", "ssh", …
    pub tag: Option<String>,
    /// A `.code-workspace` file (local or remote), opened with `--file-uri` rather than `--folder-uri`.
    pub is_workspace: bool,
    #[serde(serialize_with = "millis")]
    pub last_opened: Option<SystemTime>,
    /// Profile id from `profileAssociations` (`__default__profile__`, `4a1b9c7e`, …).
    pub profile_id: Option<String>,
}

fn millis<S: Serializer>(t: &Option<SystemTime>, s: S) -> Result<S::Ok, S::Error> {
    match t.and_then(|t| t.duration_since(UNIX_EPOCH).ok()) {
        Some(d) => s.serialize_some(&(d.as_millis() as u64)),
        None => s.serialize_none(),
    }
}

impl Project {
    /// Builds a project from a URI, or None for URIs that aren't folders/workspaces we understand.
    /// `is_workspace` comes from where the editor recorded the URI (`workspace` vs `folder` in
    /// workspace.json); without it, a `.code-workspace` suffix decides.
    pub fn make(uri: &str, paths: &Paths, is_workspace: Option<bool>) -> Option<Project> {
        let u = VsUri::parse(uri)?;
        let workspace = is_workspace.unwrap_or_else(|| u.path.ends_with(".code-workspace"));
        let base = |p: &str| -> String {
            let name = p.rsplit(['/', '\\']).find(|s| !s.is_empty()).unwrap_or("");
            if name.is_empty() { p.to_string() } else { name.to_string() }
        };
        // A workspace file is named without its extension.
        let name = |p: &str| -> String {
            let b = base(p);
            if workspace { b.strip_suffix(".code-workspace").map(str::to_string).unwrap_or(b) } else { b }
        };
        let project = |kind, base_name, detail, host_path, tag: Option<&str>| Project {
            uri: uri.to_string(),
            kind,
            base_name,
            detail,
            host_path,
            tag: tag.map(str::to_string),
            is_workspace: workspace,
            last_opened: None,
            profile_id: None,
        };

        match u.scheme.as_str() {
            "file" => {
                if u.path.is_empty() {
                    return None;
                }
                let host = u.fs_path(paths.os);
                let detail = paths.tilde(&host);
                Some(if workspace {
                    project(ProjectKind::WorkspaceFile, name(&u.path), detail, Some(host), Some("workspace"))
                } else {
                    project(ProjectKind::Folder, base(&u.path), detail, Some(host), None)
                })
            }
            "vscode-remote" => Some(match RemoteTarget::parse(&u.authority) {
                RemoteTarget::AttachedContainer { container_name } => project(
                    ProjectKind::AttachedContainer,
                    if workspace { name(&u.path) } else { container_short_name(&container_name) },
                    format!("container {container_name} · {}", u.path),
                    None,
                    Some("container"),
                ),
                RemoteTarget::DevContainer { host_path } => project(
                    ProjectKind::DevContainer,
                    if workspace {
                        name(&u.path)
                    } else {
                        host_path.as_deref().map(base).unwrap_or_else(|| base(&u.path))
                    },
                    host_path.as_deref().map(|h| paths.tilde(h)).unwrap_or_else(|| u.path.clone()),
                    host_path,
                    Some("dev container"),
                ),
                RemoteTarget::Ssh { host } => {
                    project(ProjectKind::Ssh, name(&u.path), format!("{host}:{}", u.path), None, Some("ssh"))
                }
                RemoteTarget::Wsl { distro } => {
                    project(ProjectKind::Wsl, name(&u.path), format!("WSL {distro}:{}", u.path), None, Some("wsl"))
                }
                RemoteTarget::Other { kind } => {
                    project(ProjectKind::Remote, name(&u.path), format!("{kind} {}", u.path), None, Some(&kind))
                }
            }),
            scheme => {
                Some(project(ProjectKind::Remote, name(&u.path), format!("{scheme}:{}", u.path), None, Some(scheme)))
            }
        }
    }

    /// Whether the project still exists on disk. Remote projects are assumed to exist.
    pub fn exists(&self) -> bool {
        match self.kind {
            ProjectKind::Folder | ProjectKind::WorkspaceFile => {
                self.host_path.as_deref().is_some_and(|p| Path::new(p).exists())
            }
            _ => true,
        }
    }

    /// Folder to run git in.
    pub fn git_path(&self) -> Option<String> {
        let host = self.host_path.as_deref()?;
        if self.kind == ProjectKind::WorkspaceFile {
            Path::new(host).parent().map(|p| p.to_string_lossy().into_owned())
        } else {
            Some(host.to_string())
        }
    }
}

/// Compose names its containers `<project>-<service>-<n>`: `shop-shop.dev-1` → `shop`.
pub fn container_short_name(name: &str) -> String {
    let mut n = name;
    if let Some(dash) = n.rfind('-') {
        let tail = &n[dash + 1..];
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            n = &n[..dash];
        }
    }
    if let Some(dash) = n.find('-').filter(|&i| i > 0) {
        n = &n[..dash];
    }
    if n.is_empty() { name.to_string() } else { n.to_string() }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfo {
    pub id: String,
    pub version: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// `__default__profile__` or the profile's `location` (`4a1b9c7e`, `builtin/agents`).
    pub id: String,
    /// Shown in the app. For the Default profile this is always "Default".
    pub name: String,
    /// What `--profile` expects: the profile's name as the editor knows it. The Default profile's name is
    /// translated when the editor runs with a language pack, and `--profile` creates any name it doesn't know.
    pub cli_name: String,
    /// Codicon name chosen in the editor's profile editor, if any.
    pub icon: Option<String>,
    pub is_builtin: bool,
    pub directory: PathBuf,
    /// The profile shares the Default profile's extensions (`useDefaultFlags.extensions`).
    pub uses_default_extensions: bool,
    /// The profile's own files, or the Default profile's when it shares them.
    pub settings_file: PathBuf,
    pub keybindings_file: PathBuf,
    pub extensions: Vec<ExtensionInfo>,
}

impl Profile {
    pub const DEFAULT_ID: &'static str = "__default__profile__";

    pub fn is_default(&self) -> bool {
        self.id == Self::DEFAULT_ID
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Env, Os, VSCODE};

    const SHOP: &str = "vscode-remote://attached-container%2B7b22636f6e7461696e65724e616d65223a222f73686f702d73686f702e6465762d31222c2273657474696e6773223a7b22636f6e74657874223a226f7262737461636b227d7d/app";

    #[test]
    fn project_names() {
        let paths = Paths::new(Os::Macos, &VSCODE, Env { home: "/Users/me".into(), ..Env::default() });
        let shop = Project::make(SHOP, &paths, None).unwrap();
        assert_eq!(shop.base_name, "shop");
        assert_eq!(shop.tag.as_deref(), Some("container"));
        assert_eq!(shop.kind, ProjectKind::AttachedContainer);
        assert_eq!(shop.host_path, None);

        let folder = Project::make("file:///Users/me/Documents/Projects/acme/shop", &paths, None).unwrap();
        assert_eq!(folder.base_name, "shop");
        assert_eq!(folder.detail, "~/Documents/Projects/acme/shop");
        assert!(!folder.is_workspace);

        let ws = Project::make("file:///Users/me/x/team.code-workspace", &paths, None).unwrap();
        assert_eq!(ws.kind, ProjectKind::WorkspaceFile);
        assert_eq!(ws.base_name, "team");
        assert_eq!(ws.git_path().as_deref(), Some("/Users/me/x"));
        assert!(ws.is_workspace);

        // workspace.json said "workspace": opened as a file even when remote.
        let remote =
            Project::make("vscode-remote://ssh-remote%2Bdevbox/home/me/team.code-workspace", &paths, Some(true))
                .unwrap();
        assert_eq!(remote.kind, ProjectKind::Ssh);
        assert!(remote.is_workspace);
        assert_eq!(remote.base_name, "team");

        assert_eq!(container_short_name("billing-dev-1"), "billing");
        assert_eq!(container_short_name("postgres"), "postgres");
    }

    #[test]
    fn windows_projects() {
        let paths = Paths::new(Os::Windows, &VSCODE, Env { home: r"C:\Users\Me".into(), ..Env::default() });
        let p = Project::make("file:///c%3A/Users/Me/Projects/shop", &paths, None).unwrap();
        assert_eq!(p.host_path.as_deref(), Some(r"c:\Users\Me\Projects\shop"));
        assert_eq!(p.detail, r"~\Projects\shop");
        assert_eq!(p.base_name, "shop");
        let wsl = Project::make("vscode-remote://wsl%2BUbuntu/home/me/app", &paths, None).unwrap();
        assert_eq!((wsl.kind, wsl.base_name.as_str(), wsl.tag.as_deref()), (ProjectKind::Wsl, "app", Some("wsl")));
    }
}
