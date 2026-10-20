//! Where things live on each OS, for each VS Code build. Everything is computed from strings, so the
//! Windows and Linux rules are unit-tested on any machine.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Macos,
    Windows,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::Macos
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }

    pub fn separator(self) -> char {
        if self == Os::Windows { '\\' } else { '/' }
    }
}

/// A VS Code build. They differ only in names and locations (PLAN-cross-platform.md, "Editors").
#[derive(Debug, PartialEq, Eq)]
pub struct Editor {
    pub id: &'static str,
    pub name: &'static str,
    /// User-data folder name: `~/Library/Application Support/<x>`, `%APPDATA%\<x>`, `~/.config/<x>`.
    pub data_folder: &'static str,
    /// Home-folder dot folder with `extensions/` and `argv.json`.
    pub dot_folder: &'static str,
    /// CLI name (`bin/<cli>` in the install, `<cli>.cmd` on Windows).
    pub cli: &'static str,
    pub mac_app: &'static str,
    /// Folder under `%LOCALAPPDATA%\Programs` (user install) or `%ProgramFiles%` (system install).
    pub windows_dir: &'static str,
    pub windows_exe: &'static str,
    /// Install folders, in the order they're tried.
    pub linux_roots: &'static [&'static str],
}

pub const VSCODE: Editor = Editor {
    id: "vscode",
    name: "VS Code",
    data_folder: "Code",
    dot_folder: ".vscode",
    cli: "code",
    mac_app: "Visual Studio Code.app",
    windows_dir: "Microsoft VS Code",
    windows_exe: "Code.exe",
    linux_roots: &["/usr/share/code", "/opt/visual-studio-code", "/snap/code/current/usr/share/code"],
};

/// Arch's `code` package (and other distro builds of the open-source repo). Runs on the system Electron
/// from `/usr/lib/code`; the CLI is `/usr/bin/code`.
pub const CODE_OSS: Editor = Editor {
    id: "code-oss",
    name: "Code - OSS",
    data_folder: "Code - OSS",
    dot_folder: ".vscode-oss",
    cli: "code",
    mac_app: "Code - OSS.app",
    windows_dir: "Code - OSS",
    windows_exe: "Code - OSS.exe",
    linux_roots: &["/usr/lib/code", "/usr/lib/code-oss"],
};

// Not tested on a real install yet: table entries for when someone needs them.
pub const VSCODE_INSIDERS: Editor = Editor {
    id: "vscode-insiders",
    name: "VS Code Insiders",
    data_folder: "Code - Insiders",
    dot_folder: ".vscode-insiders",
    cli: "code-insiders",
    mac_app: "Visual Studio Code - Insiders.app",
    windows_dir: "Microsoft VS Code Insiders",
    windows_exe: "Code - Insiders.exe",
    linux_roots: &["/usr/share/code-insiders", "/opt/visual-studio-code-insiders"],
};

pub const VSCODIUM: Editor = Editor {
    id: "vscodium",
    name: "VSCodium",
    data_folder: "VSCodium",
    dot_folder: ".vscode-oss",
    cli: "codium",
    mac_app: "VSCodium.app",
    windows_dir: "VSCodium",
    windows_exe: "VSCodium.exe",
    linux_roots: &["/usr/share/codium", "/opt/vscodium-bin", "/usr/lib/codium"],
};

pub const CURSOR: Editor = Editor {
    id: "cursor",
    name: "Cursor",
    data_folder: "Cursor",
    dot_folder: ".cursor",
    cli: "cursor",
    mac_app: "Cursor.app",
    windows_dir: "cursor",
    windows_exe: "Cursor.exe",
    linux_roots: &["/opt/Cursor", "/usr/share/cursor"],
};

pub const EDITORS: &[&Editor] = &[&VSCODE, &CODE_OSS, &VSCODE_INSIDERS, &VSCODIUM, &CURSOR];

pub fn editor_by_id(id: &str) -> Option<&'static Editor> {
    EDITORS.iter().copied().find(|e| e.id == id)
}

/// The environment values locations depend on.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub home: String,
    /// Windows `%APPDATA%` (Roaming).
    pub app_data: Option<String>,
    /// Windows `%LOCALAPPDATA%`.
    pub local_app_data: Option<String>,
    /// Windows `%ProgramFiles%`.
    pub program_files: Option<String>,
    /// Linux `$XDG_CONFIG_HOME`.
    pub xdg_config_home: Option<String>,
    /// Linux `$XDG_DATA_HOME`.
    pub xdg_data_home: Option<String>,
}

impl Env {
    pub fn current() -> Env {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let home = if cfg!(windows) { var("USERPROFILE") } else { var("HOME") }.unwrap_or_default();
        Env {
            home,
            app_data: var("APPDATA"),
            local_app_data: var("LOCALAPPDATA"),
            program_files: var("ProgramFiles"),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            xdg_data_home: var("XDG_DATA_HOME"),
        }
    }
}

/// Every file location the app reads or writes. Tests build it for any OS and a temporary home.
#[derive(Debug, Clone)]
pub struct Paths {
    pub os: Os,
    pub editor: &'static Editor,
    pub env: Env,
}

impl Paths {
    pub fn new(os: Os, editor: &'static Editor, env: Env) -> Paths {
        Paths { os, editor, env }
    }

    pub fn current(editor: &'static Editor) -> Paths {
        Paths::new(Os::current(), editor, Env::current())
    }

    pub fn home(&self) -> &str {
        &self.env.home
    }

    /// Joins with this OS's separator. Parts may themselves contain `/` (e.g. `builtin/agents`).
    pub fn join(&self, base: &str, parts: &[&str]) -> String {
        let sep = self.os.separator();
        // A root base ("/") trims to "", and the parts add their own leading separator.
        let mut out = base.trim_end_matches(['/', '\\']).to_string();
        for part in parts {
            for piece in part.split(['/', '\\']).filter(|p| !p.is_empty()) {
                out.push(sep);
                out.push_str(piece);
            }
        }
        out
    }

    fn at(&self, base: &str, parts: &[&str]) -> PathBuf {
        PathBuf::from(self.join(base, parts))
    }

    // MARK: editor (VS Code) locations

    /// The editor's user-data folder (`storage.json`, `workspaceStorage`, `profiles`, `languagepacks.json`).
    pub fn code_data(&self) -> PathBuf {
        let folder = self.editor.data_folder;
        match self.os {
            Os::Macos => self.at(self.home(), &["Library/Application Support", folder]),
            Os::Windows => {
                let roaming =
                    self.env.app_data.clone().unwrap_or_else(|| self.join(self.home(), &["AppData", "Roaming"]));
                self.at(&roaming, &[folder])
            }
            Os::Linux => {
                let config = self.env.xdg_config_home.clone().unwrap_or_else(|| self.join(self.home(), &[".config"]));
                self.at(&config, &[folder])
            }
        }
    }

    pub fn code_user(&self) -> PathBuf {
        self.code_data().join("User")
    }

    pub fn storage_json(&self) -> PathBuf {
        self.code_user().join("globalStorage").join("storage.json")
    }

    pub fn global_state_db(&self) -> PathBuf {
        self.code_user().join("globalStorage").join("state.vscdb")
    }

    pub fn workspace_storage(&self) -> PathBuf {
        self.code_user().join("workspaceStorage")
    }

    pub fn language_packs_json(&self) -> PathBuf {
        self.code_data().join("languagepacks.json")
    }

    pub fn default_extensions_dir(&self) -> PathBuf {
        self.at(self.home(), &[self.editor.dot_folder, "extensions"])
    }

    /// `argv.json`: `locale` decides VS Code's display language.
    pub fn argv_json(&self) -> PathBuf {
        self.at(self.home(), &[self.editor.dot_folder, "argv.json"])
    }

    // MARK: our own state, shared with the dev-layout extension: ~/.config/code-toolbox on every OS

    pub fn config_dir(&self) -> PathBuf {
        self.at(self.home(), &[".config", "code-toolbox"])
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir().join("config.json")
    }

    pub fn open_windows_dir(&self) -> PathBuf {
        self.config_dir().join("open")
    }

    pub fn skip_layout_dir(&self) -> PathBuf {
        self.config_dir().join("skip-layout")
    }

    pub fn close_dir(&self) -> PathBuf {
        self.config_dir().join("close")
    }

    // MARK: JetBrains Toolbox (favorites import)

    pub fn toolbox_settings(&self) -> PathBuf {
        let rel = ["JetBrains", "Toolbox", ".settings.json"];
        match self.os {
            Os::Macos => self.at(self.home(), &["Library/Application Support", rel[0], rel[1], rel[2]]),
            Os::Windows => {
                let local =
                    self.env.local_app_data.clone().unwrap_or_else(|| self.join(self.home(), &["AppData", "Local"]));
                self.at(&local, &rel)
            }
            Os::Linux => {
                let data =
                    self.env.xdg_data_home.clone().unwrap_or_else(|| self.join(self.home(), &[".local", "share"]));
                self.at(&data, &rel)
            }
        }
    }

    // MARK: display

    /// `/Users/me/x` → `~/x` (`C:\Users\Me\x` → `~\x`, ignoring case on Windows).
    pub fn tilde(&self, path: &str) -> String {
        let home = self.home().trim_end_matches(['/', '\\']);
        if home.is_empty() {
            return path.to_string();
        }
        let (p, h) = if self.os == Os::Windows {
            (path.to_lowercase(), home.to_lowercase())
        } else {
            (path.to_string(), home.to_string())
        };
        if p == h {
            return "~".into();
        }
        let sep = self.os.separator();
        if p.starts_with(&format!("{h}{sep}")) || (self.os == Os::Windows && p.starts_with(&format!("{h}/"))) {
            // Byte offsets match: lowercasing these paths doesn't change their length in practice.
            return format!("~{}", &path[home.len()..]);
        }
        path.to_string()
    }

    /// `~/x` → `/Users/me/x` (also `~\x` on Windows).
    pub fn expand_tilde(&self, path: &str) -> String {
        if path == "~" {
            return self.home().to_string();
        }
        if let Some(rest) =
            path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\").filter(|_| self.os == Os::Windows))
        {
            return self.join(self.home(), &[rest]);
        }
        path.to_string()
    }

    /// A path in the form to compare by: on Windows, lowercased with backslashes (drive letters come in
    /// both cases: VS Code lowercases them, %USERPROFILE% doesn't); elsewhere, unchanged.
    pub fn path_key(&self, path: &str) -> String {
        if self.os == Os::Windows {
            path.replace('/', "\\").trim_end_matches('\\').to_lowercase()
        } else {
            path.trim_end_matches('/').to_string()
        }
    }

    /// Same file on this OS? (Windows paths ignore case and separator style.)
    pub fn same_path(&self, a: &str, b: &str) -> bool {
        self.path_key(a) == self.path_key(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(home: &str) -> Env {
        Env { home: home.into(), ..Env::default() }
    }

    #[test]
    fn mac_locations() {
        let p = Paths::new(Os::Macos, &VSCODE, env("/Users/me"));
        assert_eq!(
            p.storage_json().to_string_lossy(),
            "/Users/me/Library/Application Support/Code/User/globalStorage/storage.json"
        );
        assert_eq!(p.default_extensions_dir().to_string_lossy(), "/Users/me/.vscode/extensions");
        assert_eq!(p.config_file().to_string_lossy(), "/Users/me/.config/code-toolbox/config.json");
        assert_eq!(p.tilde("/Users/me/Documents/x"), "~/Documents/x");
        assert_eq!(p.tilde("/Users/meow/x"), "/Users/meow/x");
        assert_eq!(p.expand_tilde("~/Downloads/**"), "/Users/me/Downloads/**");
    }

    #[test]
    fn linux_locations_for_code_oss() {
        let p = Paths::new(Os::Linux, &CODE_OSS, env("/home/me"));
        assert_eq!(p.code_data().to_string_lossy(), "/home/me/.config/Code - OSS");
        assert_eq!(p.argv_json().to_string_lossy(), "/home/me/.vscode-oss/argv.json");
        assert_eq!(p.toolbox_settings().to_string_lossy(), "/home/me/.local/share/JetBrains/Toolbox/.settings.json");
        let xdg = Paths::new(Os::Linux, &VSCODE, Env { xdg_config_home: Some("/cfg".into()), ..env("/home/me") });
        assert_eq!(xdg.code_data().to_string_lossy(), "/cfg/Code");
    }

    #[test]
    fn windows_locations() {
        let e = Env {
            app_data: Some(r"C:\Users\Me\AppData\Roaming".into()),
            local_app_data: Some(r"C:\Users\Me\AppData\Local".into()),
            ..env(r"C:\Users\Me")
        };
        let p = Paths::new(Os::Windows, &VSCODE, e);
        assert_eq!(p.join(&p.code_data().to_string_lossy(), &["User"]), r"C:\Users\Me\AppData\Roaming\Code\User");
        assert_eq!(p.config_dir().to_string_lossy(), r"C:\Users\Me\.config\code-toolbox");
        assert_eq!(
            p.toolbox_settings().to_string_lossy(),
            r"C:\Users\Me\AppData\Local\JetBrains\Toolbox\.settings.json"
        );
        assert_eq!(p.tilde(r"c:\users\me\Projects\x"), r"~\Projects\x");
        assert_eq!(p.expand_tilde("~/Downloads/**"), r"C:\Users\Me\Downloads\**");
        assert!(p.same_path(r"c:\Users\me\x", "C:/Users/Me/x/"));
        assert_eq!(p.path_key(r"C:\Users\Me\Projects\shop"), p.path_key(r"c:\Users\Me\Projects\shop"));
    }
}
