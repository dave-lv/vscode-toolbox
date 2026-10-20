//! `~/.config/code-toolbox/config.json`: hand-editable. Missing keys fall back to defaults, unknown keys
//! are kept, and saves keep the format the original Swift app wrote (Apple's pretty JSON: sorted keys,
//! `" : "`, empty collections as `[\n\n  ]`), so existing files (e.g. in a dotfiles repo) don't churn.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::platform::Paths;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScanConfig {
    pub enabled: bool,
    pub roots: Vec<String>,
    pub depth: usize,
    /// Keys this version doesn't know: kept on save, like `Config::extra`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        ScanConfig { enabled: false, roots: vec!["~/Documents/Projects".into()], depth: 2, extra: Map::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Editor URIs, in display order.
    pub favorites: Vec<String>,
    pub hidden: Vec<String>,
    /// URI → display name.
    pub aliases: BTreeMap<String, String>,
    /// URI → the dev-layout extension's agent for that project ("claude" or "codex"), over the profile's
    /// `devLayout.agent`. The extension reads it from here. Only written once used.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub agents: BTreeMap<String, String>,
    pub exclude: Vec<String>,
    pub show_builtin_profiles: bool,
    pub show_git_status: bool,
    pub scan: ScanConfig,
    /// macOS: the editor app to use. Ignored on Windows and Linux.
    pub code_app: String,
    /// Keys this version doesn't know (written by a newer version, or by hand): kept on save.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            favorites: vec![],
            hidden: vec![],
            aliases: BTreeMap::new(),
            agents: BTreeMap::new(),
            exclude: vec!["~/Downloads/**".into(), "~/Library/**".into()],
            show_builtin_profiles: false,
            show_git_status: true,
            scan: ScanConfig::default(),
            code_app: "/Applications/Visual Studio Code.app".into(),
            extra: Map::new(),
        }
    }
}

// Returned once per load and matched right away, never stored: boxing the config would buy nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum LoadResult {
    Loaded(Config),
    /// No file yet: safe to create one.
    Missing,
    /// The file exists but couldn't be read or parsed: report it, never overwrite it.
    Invalid(String),
}

pub struct ConfigStore;

impl ConfigStore {
    pub fn load(paths: &Paths) -> LoadResult {
        Self::load_with_bytes(paths).0
    }

    /// Also returns the bytes the config was parsed from, for `save_if_unchanged`.
    pub fn load_with_bytes(paths: &Paths) -> (LoadResult, Option<Vec<u8>>) {
        let file = paths.config_file();
        let bytes = match fs::read(&file) {
            Ok(b) => b,
            // Only "doesn't exist" counts as missing (a dangling symlink isn't missing either).
            Err(e) => {
                let result = if fs::symlink_metadata(&file).is_ok() {
                    LoadResult::Invalid(format!("Couldn't read config.json: {e}."))
                } else {
                    LoadResult::Missing
                };
                return (result, None);
            }
        };
        let result = match serde_json::from_slice(&bytes) {
            Ok(c) => LoadResult::Loaded(c),
            Err(e) => LoadResult::Invalid(format!("config.json is not valid JSON: {e}.")),
        };
        (result, Some(bytes))
    }

    /// Saves only if config.json still holds `expected` (None: still doesn't exist), so a change made on
    /// disk since it was read (by hand, or another copy of the app) isn't overwritten. Ok(false) means it
    /// changed: reload and redo the change. The check and the replace aren't one atomic step, but the
    /// window is a single read.
    pub fn save_if_unchanged(config: &Config, paths: &Paths, expected: Option<&[u8]>) -> io::Result<bool> {
        if fs::read(paths.config_file()).ok().as_deref() != expected {
            return Ok(false);
        }
        Self::save(config, paths)?;
        Ok(true)
    }

    /// The file config.json stands for: itself, or what it links to (followed through a chain of
    /// symlinks, also when the final target doesn't exist yet). Writing there keeps the link intact.
    pub fn target(paths: &Paths) -> PathBuf {
        let mut path = paths.config_file();
        for _ in 0..16 {
            let Ok(dest) = fs::read_link(&path) else { break };
            path = if dest.is_absolute() { dest } else { path.parent().unwrap_or(Path::new("/")).join(dest) };
            path = normalize(&path);
        }
        path
    }

    /// Atomically replaces the config file (through a symlink, if config.json is one).
    pub fn save(config: &Config, paths: &Paths) -> io::Result<()> {
        fs::create_dir_all(paths.config_dir())?;
        let file = Self::target(paths);
        let dir = file.parent().map(Path::to_path_buf).unwrap_or_else(|| paths.config_dir());
        fs::create_dir_all(&dir)?;
        let mut text = to_apple_json(&serde_json::to_value(config).map_err(io::Error::other)?);
        text.push('\n');
        let tmp = dir.join(format!(
            ".{}.tmp-{}",
            file.file_name().and_then(|n| n.to_str()).unwrap_or("config.json"),
            std::process::id()
        ));
        fs::write(&tmp, text)?;
        fs::rename(&tmp, &file).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }

    /// For a config.json that can't be read or parsed: moves it (the link's target, if it's a link)
    /// aside to `<name>.broken` and saves `config` in its place. Returns where the old one went.
    pub fn replace_broken(config: &Config, paths: &Paths) -> io::Result<Option<PathBuf>> {
        let file = Self::target(paths);
        let mut backup = None;
        if fs::symlink_metadata(&file).is_ok() {
            let mut name = file.file_name().unwrap_or_default().to_os_string();
            name.push(".broken");
            let dest = file.with_file_name(name);
            if fs::symlink_metadata(&dest).is_ok() {
                if dest.is_dir() { fs::remove_dir_all(&dest)? } else { fs::remove_file(&dest)? }
            }
            fs::rename(&file, &dest)?;
            backup = Some(dest);
        }
        Self::save(config, paths)?;
        Ok(backup)
    }
}

/// `a/./b/../c` → `a/c` without touching the filesystem (the target may not exist).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Apple's `JSONEncoder` with `.prettyPrinted, .sortedKeys, .withoutEscapingSlashes`.
pub fn to_apple_json(value: &Value) -> String {
    let mut out = String::new();
    write_apple(value, 0, &mut out);
    out
}

fn write_apple(value: &Value, level: usize, out: &mut String) {
    let indent = |n: usize| "  ".repeat(n);
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str(&format!("{{\n\n{}}}", indent(level)));
                return;
            }
            out.push_str("{\n");
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                out.push_str(&indent(level + 1));
                out.push_str(&serde_json::to_string(key).unwrap_or_default());
                out.push_str(" : ");
                write_apple(&map[*key], level + 1, out);
            }
            out.push_str(&format!("\n{}}}", indent(level)));
        }
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str(&format!("[\n\n{}]", indent(level)));
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                out.push_str(&indent(level + 1));
                write_apple(item, level + 1, out);
            }
            out.push_str(&format!("\n{}]", indent(level)));
        }
        other => out.push_str(&serde_json::to_string(other).unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerates_missing_keys_and_keeps_unknown_ones() {
        let c: Config =
            serde_json::from_str(r#"{"favorites":["file:///a"],"editor":{"id":"code-oss"},"scan":{"exclude":["x"]}}"#)
                .unwrap();
        assert_eq!(c.scan.extra["exclude"], serde_json::json!(["x"]));
        assert!(serde_json::to_value(&c).unwrap()["scan"]["exclude"].is_array(), "nested unknown key dropped");
        assert_eq!(c.favorites, ["file:///a"]);
        assert_eq!(c.exclude, ["~/Downloads/**", "~/Library/**"]);
        assert!(!c.scan.enabled);
        assert_eq!(c.extra["editor"]["id"], "code-oss");
        assert!(
            to_apple_json(&serde_json::to_value(&c).unwrap())
                .contains("\"editor\" : {\n    \"id\" : \"code-oss\"\n  }")
        );
        // A wrong type is invalid, as in the Swift app.
        assert!(serde_json::from_str::<Config>(r#"{"favorites":"x"}"#).is_err());
    }

    /// Exactly what the Swift app wrote for the real config (checked byte for byte on the Mac).
    #[test]
    fn writes_apple_pretty_json() {
        let c = Config {
            favorites: vec!["file:///Users/me/Documents/Projects/acme/shop".into()],
            ..Config::default()
        };
        let expected = r#"{
  "aliases" : {

  },
  "codeApp" : "/Applications/Visual Studio Code.app",
  "exclude" : [
    "~/Downloads/**",
    "~/Library/**"
  ],
  "favorites" : [
    "file:///Users/me/Documents/Projects/acme/shop"
  ],
  "hidden" : [

  ],
  "scan" : {
    "depth" : 2,
    "enabled" : false,
    "roots" : [
      "~/Documents/Projects"
    ]
  },
  "showBuiltinProfiles" : false,
  "showGitStatus" : true
}"#;
        assert_eq!(to_apple_json(&serde_json::to_value(&c).unwrap()), expected);
    }

    #[test]
    fn agents_written_once_used() {
        let mut c = Config::default();
        c.agents.insert("file:///a".into(), "codex".into());
        let text = to_apple_json(&serde_json::to_value(&c).unwrap());
        assert!(text.contains("\"agents\" : {\n    \"file:///a\" : \"codex\"\n  }"), "{text}");
        assert_eq!(serde_json::from_str::<Config>(&text).unwrap(), c);
    }
}
