//! Reads the editor's on-disk state. None of these files are a public API, so each source is parsed
//! separately and a failure only drops that source.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::install::Installation;
use crate::jsonc;
use crate::models::{ExtensionInfo, Profile, Project};
use crate::platform::Paths;
use crate::uri::fs_path_from_uri_path;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Every folder/workspace the editor has opened, with last-opened time and profile.
    pub projects: Vec<Project>,
    /// Default first, then the editor's order.
    pub profiles: Vec<Profile>,
    pub version: Option<String>,
    /// Sources that failed to load; the rest is still shown.
    pub warnings: Vec<String>,
}

pub struct VsCodeReader<'a> {
    pub paths: &'a Paths,
    pub installation: Option<&'a Installation>,
    /// The OS UI language, used when argv.json doesn't set `locale` (e.g. `en-US`).
    pub system_locale: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct HistoryEntry {
    last_opened: Option<SystemTime>,
    /// Recorded as a workspace file rather than a folder.
    is_workspace: bool,
}

impl VsCodeReader<'_> {
    pub fn read(&self) -> Snapshot {
        let mut warnings = Vec::new();
        let storage = read_object(&self.paths.storage_json());
        if storage.is_none() {
            warnings.push(format!("Couldn't read {}", self.paths.tilde(&self.paths.storage_json().to_string_lossy())));
        }

        let mut history = self.read_workspace_storage();
        for (uri, is_workspace) in self.read_recently_opened() {
            history.entry(uri).or_insert(HistoryEntry { last_opened: None, is_workspace });
        }

        let associations: HashMap<String, String> = storage
            .as_ref()
            .and_then(|s| s.get("profileAssociations")?.get("workspaces")?.as_object().cloned())
            .map(|m| m.into_iter().filter_map(|(k, v)| Some((k, v.as_str()?.to_string()))).collect())
            .unwrap_or_default();

        let projects = history
            .into_iter()
            .filter_map(|(uri, entry)| {
                let mut p = Project::make(&uri, self.paths, Some(entry.is_workspace))?;
                p.last_opened = entry.last_opened;
                p.profile_id = associations.get(&uri).cloned();
                Some(p)
            })
            .collect();

        Snapshot {
            projects,
            profiles: self.read_profiles(storage.as_ref()),
            version: self.installation.and_then(Installation::version),
            warnings,
        }
    }

    /// `workspaceStorage/<hash>/workspace.json` exists for every folder/workspace ever opened; the
    /// folder's `state.vscdb` is rewritten while the window is open, so its mtime is "last opened".
    fn read_workspace_storage(&self) -> HashMap<String, HistoryEntry> {
        let mut result: HashMap<String, HistoryEntry> = HashMap::new();
        let Ok(dirs) = fs::read_dir(self.paths.workspace_storage()) else { return result };
        for dir in dirs.flatten().map(|d| d.path()) {
            let Some(json) = read_object(&dir.join("workspace.json")) else { continue };
            let (uri, is_workspace) =
                match (json.get("folder").and_then(Value::as_str), json.get("workspace").and_then(Value::as_str)) {
                    (Some(folder), _) => (folder.to_string(), false),
                    (None, Some(ws)) => (ws.to_string(), true),
                    _ => continue,
                };
            let date = [dir.join("state.vscdb"), dir.clone()]
                .iter()
                .filter_map(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
                .max();
            let entry = result.entry(uri).or_insert(HistoryEntry { last_opened: None, is_workspace });
            entry.last_opened = entry.last_opened.max(date);
            entry.is_workspace = is_workspace;
        }
        result
    }

    /// The editor's own recent list (`history.recentlyOpenedPathsList`). Missing on some installs.
    fn read_recently_opened(&self) -> Vec<(String, bool)> {
        let db = self.paths.global_state_db();
        if !db.exists() {
            return vec![];
        }
        let Some(value) = sqlite_value(&db, "history.recentlyOpenedPathsList") else { return vec![] };
        let Ok(json) = serde_json::from_str::<Value>(&value) else { return vec![] };
        json.get("entries")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| {
                        if let Some(folder) = e.get("folderUri").and_then(Value::as_str) {
                            return Some((folder.to_string(), false));
                        }
                        let ws = e.get("workspace")?.get("configPath")?.as_str()?;
                        Some((ws.to_string(), true))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn read_profiles(&self, storage: Option<&Map<String, Value>>) -> Vec<Profile> {
        let user = self.paths.code_user();
        let default_extensions = self.read_extensions(&self.paths.default_extensions_dir().join("extensions.json"));
        let default = Profile {
            id: Profile::DEFAULT_ID.into(),
            name: "Default".into(),
            cli_name: self.default_profile_name(),
            icon: None,
            is_builtin: false,
            directory: user.clone(),
            uses_default_extensions: false,
            settings_file: user.join("settings.json"),
            keybindings_file: user.join("keybindings.json"),
            extensions: default_extensions.clone(),
        };
        let mut profiles = vec![default.clone()];

        let entries = storage.and_then(|s| s.get("userDataProfiles")).and_then(Value::as_array);
        for entry in entries.into_iter().flatten() {
            let (Some(location), Some(name)) =
                (entry.get("location").and_then(Value::as_str), entry.get("name").and_then(Value::as_str))
            else {
                continue;
            };
            let dir = location.split('/').fold(user.join("profiles"), |d, part| d.join(part));
            let flag =
                |k: &str| entry.get("useDefaultFlags").and_then(|f| f.get(k)).and_then(Value::as_bool).unwrap_or(false);
            let shares_extensions = flag("extensions");
            profiles.push(Profile {
                id: location.into(),
                name: name.into(),
                cli_name: name.into(),
                icon: entry.get("icon").and_then(Value::as_str).map(str::to_string),
                is_builtin: location.starts_with("builtin/"),
                directory: dir.clone(),
                uses_default_extensions: shares_extensions,
                settings_file: if flag("settings") { default.settings_file.clone() } else { dir.join("settings.json") },
                keybindings_file: if flag("keybindings") {
                    default.keybindings_file.clone()
                } else {
                    dir.join("keybindings.json")
                },
                extensions: if shares_extensions {
                    default_extensions.clone()
                } else {
                    self.read_extensions(&dir.join("extensions.json"))
                },
            });
        }
        profiles
    }

    /// The Default profile's name as the editor knows it, which `--profile` matches against. It's
    /// translated when a language pack for the editor's display language is installed. Mirrors the
    /// editor's own lookup: argv.json `locale`, else the OS language; English unless languagepacks.json has
    /// a pack for it (trying `de-ch`, then `de`); the pack's translation of `defaultProfile`, else English.
    pub fn default_profile_name(&self) -> String {
        const ENGLISH: &str = "Default";
        let argv = fs::read_to_string(self.paths.argv_json()).ok().and_then(|t| jsonc::parse_object(&t));
        let mut locale = argv
            .as_ref()
            .and_then(|a| a.get("locale")?.as_str().map(str::to_string))
            .unwrap_or_else(|| self.system_locale.clone())
            .to_lowercase();
        if locale.starts_with("zh") {
            let region = locale.split('-').nth(1).unwrap_or("");
            locale = if ["hans", "cn", "sg", "my"].contains(&region) { "zh-cn".into() } else { "zh-tw".into() };
        }
        if locale.starts_with("en") || locale == "pseudo" {
            return ENGLISH.into();
        }
        let Some(packs) = read_object(&self.paths.language_packs_json()) else { return ENGLISH.into() };
        let mut candidate = locale.as_str();
        while !packs.contains_key(candidate) {
            match candidate.rfind('-') {
                Some(dash) if dash > 0 => candidate = &candidate[..dash],
                _ => return ENGLISH.into(),
            }
        }
        packs
            .get(candidate)
            .and_then(|p| p.get("translations")?.get("vscode")?.as_str())
            .and_then(|file| read_object(Path::new(file)))
            .and_then(|t| {
                t.get("contents")?
                    .get("vs/platform/userDataProfile/common/userDataProfile")?
                    .get("defaultProfile")?
                    .as_str()
                    .map(str::to_string)
            })
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| ENGLISH.into())
    }

    fn read_extensions(&self, file: &Path) -> Vec<ExtensionInfo> {
        let Ok(text) = fs::read_to_string(file) else { return vec![] };
        let Ok(Value::Array(list)) = serde_json::from_str::<Value>(&text) else { return vec![] };
        let mut out: Vec<ExtensionInfo> = list
            .iter()
            .filter_map(|e| {
                let id = e.get("identifier")?.get("id")?.as_str()?.to_string();
                let version = e.get("version").and_then(Value::as_str).unwrap_or("").to_string();
                Some(ExtensionInfo { id, version, display_name: self.display_name(e.get("location")) })
            })
            .collect();
        out.sort_by_key(|x| x.display_name.as_deref().unwrap_or(&x.id).to_lowercase());
        out
    }

    fn display_name(&self, location: Option<&Value>) -> Option<String> {
        let os = self.paths.os;
        let path = match location? {
            // fsPath is already native; path is URI-style (`/c:/Users/…` on Windows).
            Value::Object(o) => o
                .get("fsPath")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| o.get("path").and_then(Value::as_str).map(|p| fs_path_from_uri_path(p, os)))?,
            Value::String(s) if s.starts_with("file://") => crate::uri::VsUri::parse(s)?.fs_path(os),
            Value::String(s) => s.clone(),
            _ => return None,
        };
        let pkg = read_object(&Path::new(&path).join("package.json"))?;
        let name = pkg.get("displayName")?.as_str()?;
        (!name.starts_with('%')).then(|| name.to_string())
    }
}

#[cfg(not(feature = "recent-db"))]
fn sqlite_value(_db: &Path, _key: &str) -> Option<String> {
    None
}

fn read_object(path: &Path) -> Option<Map<String, Value>> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

#[cfg(feature = "recent-db")]
fn sqlite_value(db: &Path, key: &str) -> Option<String> {
    use rusqlite::{Connection, OpenFlags};
    let conn =
        Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
    conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
        let v = row.get_ref(0)?;
        Ok(match v {
            rusqlite::types::ValueRef::Text(b) | rusqlite::types::ValueRef::Blob(b) => {
                Some(String::from_utf8_lossy(b).into_owned())
            }
            _ => None,
        })
    })
    .ok()
    .flatten()
}
