//! Finding the editor's installation: its version and how to start its CLI.

use std::fs;
use std::path::{Path, PathBuf};

use crate::platform::{Os, Paths};

/// How to run the editor's CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cli {
    /// A script or binary taking the CLI arguments (`…/bin/code`).
    Script(PathBuf),
    /// Windows: `Code.exe cli.js <args>` with `ELECTRON_RUN_AS_NODE=1`, which is what `bin\code.cmd` does.
    /// Not through the .cmd itself: folder URIs contain `%` sequences that cmd.exe would expand.
    Electron { exe: PathBuf, cli_js: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub root: PathBuf,
    /// The folder with the editor's `package.json` and `out/` (`resources/app`, or the root itself for
    /// distro builds that run on the system Electron, like Arch's Code - OSS).
    pub app_dir: PathBuf,
    pub cli: Cli,
}

impl Installation {
    /// Where to look, in order. `app_override` is the macOS app path from config.json (`codeApp`).
    pub fn candidates(paths: &Paths, app_override: Option<&str>) -> Vec<PathBuf> {
        let e = paths.editor;
        let mut roots: Vec<PathBuf> = Vec::new();
        match paths.os {
            Os::Macos => {
                if let Some(o) = app_override.map(str::trim).filter(|o| !o.is_empty()) {
                    roots.push(PathBuf::from(paths.expand_tilde(o)));
                }
                roots.push(PathBuf::from(paths.join("/Applications", &[e.mac_app])));
                roots.push(PathBuf::from(paths.join(paths.home(), &["Applications", e.mac_app])));
            }
            Os::Windows => {
                let local =
                    paths.env.local_app_data.clone().unwrap_or_else(|| paths.join(paths.home(), &["AppData", "Local"]));
                roots.push(PathBuf::from(paths.join(&local, &["Programs", e.windows_dir])));
                if let Some(pf) = &paths.env.program_files {
                    roots.push(PathBuf::from(paths.join(pf, &[e.windows_dir])));
                }
            }
            Os::Linux => roots.extend(e.linux_roots.iter().map(PathBuf::from)),
        }
        roots.dedup();
        roots
    }

    pub fn detect(paths: &Paths, app_override: Option<&str>) -> Option<Installation> {
        Self::candidates(paths, app_override).iter().find_map(|root| Self::at(root, paths))
    }

    /// The installation at `root`, if it has the expected layout.
    pub fn at(root: &Path, paths: &Paths) -> Option<Installation> {
        let cli_name = paths.editor.cli;
        match paths.os {
            Os::Macos => {
                let app_dir = root.join("Contents/Resources/app");
                let cli = app_dir.join("bin").join(cli_name);
                cli.exists().then(|| Installation { root: root.into(), app_dir, cli: Cli::Script(cli) })
            }
            Os::Windows => {
                let bin = root.join("bin");
                let from_cmd = fs::read_to_string(bin.join(format!("{cli_name}.cmd")))
                    .ok()
                    .and_then(|text| parse_code_cmd(&text, &bin.to_string_lossy()));
                let (exe, cli_js) = from_cmd.map(|(a, b)| (PathBuf::from(a), PathBuf::from(b))).unwrap_or_else(|| {
                    (root.join(paths.editor.windows_exe), root.join("resources").join("app").join("out").join("cli.js"))
                });
                if !exe.exists() {
                    return None;
                }
                let app_dir = cli_js
                    .parent()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| root.join("resources").join("app"));
                Some(Installation { root: root.into(), app_dir, cli: Cli::Electron { exe, cli_js } })
            }
            Os::Linux => {
                let bundled = root.join("resources/app");
                if bundled.join("package.json").exists() {
                    let cli = root.join("bin").join(cli_name);
                    return cli.exists().then(|| Installation {
                        root: root.into(),
                        app_dir: bundled,
                        cli: Cli::Script(cli),
                    });
                }
                // Distro build on the system Electron: the app folder is the root, the CLI is in /usr/bin.
                if root.join("package.json").exists() {
                    let cli = [PathBuf::from("/usr/bin").join(cli_name), root.join("bin").join(cli_name)]
                        .into_iter()
                        .find(|c| c.exists())?;
                    return Some(Installation { root: root.into(), app_dir: root.into(), cli: Cli::Script(cli) });
                }
                None
            }
        }
    }

    /// `version` from the app's package.json.
    pub fn version(&self) -> Option<String> {
        let text = fs::read_to_string(self.app_dir.join("package.json")).ok()?;
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        json.get("version")?.as_str().map(str::to_string)
    }

    /// The codicon font shipped with the editor (for profile icons, if not bundled).
    pub fn codicon_font(&self) -> PathBuf {
        self.app_dir.join("out/media/codicon.ttf")
    }
}

/// From `bin\code.cmd`, the two paths it runs: `"%~dp0..\Code.exe" "%~dp0..\resources\app\out\cli.js" %*`.
/// `%~dp0` is the .cmd's folder (with a trailing backslash). Returns (exe, cli.js), normalized.
pub fn parse_code_cmd(text: &str, bin_dir: &str) -> Option<(String, String)> {
    let line = text.lines().find(|l| l.contains("%~dp0") && l.contains(".exe\""))?;
    let quoted: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
    let (exe, js) = (quoted.first()?, quoted.get(1)?);
    if !js.ends_with(".js") {
        return None;
    }
    let base = format!("{}\\", bin_dir.trim_end_matches(['\\', '/']));
    let expand = |s: &str| normalize_windows(&s.replace("%~dp0", &base));
    Some((expand(exe), expand(js)))
}

/// `C:\a\bin\..\Code.exe` → `C:\a\Code.exe` (string only, so it works on any OS).
fn normalize_windows(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['\\', '/']) {
        match part {
            "" | "." if !parts.is_empty() => {}
            ".." => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            p => parts.push(p),
        }
    }
    parts.join("\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{CODE_OSS, Env, VSCODE};

    #[test]
    fn parses_code_cmd() {
        let cmd = "@echo off\r\nsetlocal\r\nset VSCODE_DEV=\r\nset ELECTRON_RUN_AS_NODE=1\r\n\"%~dp0..\\Code.exe\" \"%~dp0..\\resources\\app\\out\\cli.js\" %*\r\nIF %ERRORLEVEL% NEQ 0 EXIT /b %ERRORLEVEL%\r\nendlocal\r\n";
        let (exe, js) = parse_code_cmd(cmd, r"C:\Users\Me\AppData\Local\Programs\Microsoft VS Code\bin").unwrap();
        assert_eq!(exe, r"C:\Users\Me\AppData\Local\Programs\Microsoft VS Code\Code.exe");
        assert_eq!(js, r"C:\Users\Me\AppData\Local\Programs\Microsoft VS Code\resources\app\out\cli.js");
        // A versioned app folder works the same way.
        let versioned = "\"%~dp0..\\Code.exe\" \"%~dp0..\\1a2b3c4d\\resources\\app\\out\\cli.js\" %*";
        assert_eq!(parse_code_cmd(versioned, r"D:\VSC\bin").unwrap().1, r"D:\VSC\1a2b3c4d\resources\app\out\cli.js");
        assert_eq!(parse_code_cmd("@echo off", r"D:\VSC\bin"), None);
    }

    #[test]
    fn candidate_roots() {
        let mac = Paths::new(Os::Macos, &VSCODE, Env { home: "/Users/me".into(), ..Env::default() });
        assert_eq!(
            Installation::candidates(&mac, Some("~/Apps/Code.app")),
            [
                "/Users/me/Apps/Code.app",
                "/Applications/Visual Studio Code.app",
                "/Users/me/Applications/Visual Studio Code.app"
            ]
            .map(PathBuf::from)
        );
        let win = Paths::new(
            Os::Windows,
            &VSCODE,
            Env {
                home: r"C:\Users\Me".into(),
                local_app_data: Some(r"C:\Users\Me\AppData\Local".into()),
                program_files: Some(r"C:\Program Files".into()),
                ..Env::default()
            },
        );
        assert_eq!(
            Installation::candidates(&win, None),
            [r"C:\Users\Me\AppData\Local\Programs\Microsoft VS Code", r"C:\Program Files\Microsoft VS Code"]
                .map(PathBuf::from)
        );
        let oss = Paths::new(Os::Linux, &CODE_OSS, Env { home: "/home/me".into(), ..Env::default() });
        assert_eq!(Installation::candidates(&oss, None)[0], PathBuf::from("/usr/lib/code"));
    }

    #[cfg(unix)]
    #[test]
    fn detects_mac_and_linux_layouts() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Visual Studio Code.app");
        let app = root.join("Contents/Resources/app");
        fs::create_dir_all(app.join("bin")).unwrap();
        fs::write(app.join("bin/code"), "").unwrap();
        fs::write(app.join("package.json"), r#"{"version":"1.142.0"}"#).unwrap();
        let mac = Paths::new(Os::Macos, &VSCODE, Env { home: tmp.path().to_string_lossy().into(), ..Env::default() });
        let inst = Installation::at(&root, &mac).unwrap();
        assert_eq!(inst.version().as_deref(), Some("1.142.0"));
        assert_eq!(inst.cli, Cli::Script(app.join("bin/code")));

        // Microsoft's Linux layout (resources/app + bin/code).
        let lin = tmp.path().join("code");
        fs::create_dir_all(lin.join("resources/app")).unwrap();
        fs::create_dir_all(lin.join("bin")).unwrap();
        fs::write(lin.join("resources/app/package.json"), r#"{"version":"1.142.0"}"#).unwrap();
        fs::write(lin.join("bin/code"), "").unwrap();
        let linux = Paths::new(Os::Linux, &VSCODE, Env { home: "/home/me".into(), ..Env::default() });
        assert_eq!(Installation::at(&lin, &linux).unwrap().app_dir, lin.join("resources/app"));
    }
}
