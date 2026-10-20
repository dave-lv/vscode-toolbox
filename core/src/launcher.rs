//! Running the editor's CLI.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::install::{Cli, Installation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchRequest {
    /// Open a project. `profile_name` None lets the editor use the profile it remembers for the folder.
    /// `is_workspace` opens a `.code-workspace` file (local or remote) instead of a folder.
    Project { uri: String, is_workspace: bool, profile_name: Option<String>, new_window: bool },
    /// Empty window with a profile (Profiles tab).
    NewWindow { profile_name: String },
    /// Open a file (profile settings.json etc.).
    File { path: String, profile_name: Option<String> },
    /// Install (or replace) an extension from a .vsix. `profile_name` None: the default profile.
    InstallExtension { vsix: String, profile_name: Option<String> },
    /// Uninstall an extension by id.
    UninstallExtension { id: String, profile_name: Option<String> },
}

impl LaunchRequest {
    /// Only pass `Profile::cli_name` values: `--profile X` creates X if it doesn't exist.
    pub fn arguments(&self) -> Vec<String> {
        let profile =
            |name: &Option<String>| name.iter().flat_map(|n| ["--profile".to_string(), n.clone()]).collect::<Vec<_>>();
        match self {
            LaunchRequest::Project { uri, is_workspace, profile_name, new_window } => {
                let mut args = profile(profile_name);
                if *new_window {
                    args.push("--new-window".into());
                }
                args.push(if *is_workspace { "--file-uri" } else { "--folder-uri" }.into());
                args.push(uri.clone());
                args
            }
            LaunchRequest::NewWindow { profile_name } => {
                vec!["--profile".into(), profile_name.clone(), "--new-window".into()]
            }
            LaunchRequest::File { path, profile_name } => {
                let mut args = profile(profile_name);
                args.push(path.clone());
                args
            }
            LaunchRequest::InstallExtension { vsix, profile_name } => {
                let mut args = profile(profile_name);
                args.extend(["--install-extension".into(), vsix.clone(), "--force".into()]);
                args
            }
            LaunchRequest::UninstallExtension { id, profile_name } => {
                let mut args = profile(profile_name);
                args.extend(["--uninstall-extension".into(), id.clone()]);
                args
            }
        }
    }
}

/// The CLI process for `request`. `env` replaces the environment (the login-shell environment on macOS
/// and Linux: the editor skips its own shell-environment lookup when started from its CLI).
pub fn command(
    installation: &Installation,
    request: &LaunchRequest,
    env: Option<&HashMap<String, String>>,
    home: &Path,
) -> Command {
    let mut cmd = match &installation.cli {
        Cli::Script(path) => Command::new(path),
        Cli::Electron { exe, cli_js } => {
            let mut c = Command::new(exe);
            c.arg(cli_js);
            c
        }
    };
    cmd.args(request.arguments());
    if let Some(env) = env {
        cmd.env_clear().envs(env);
    }
    if matches!(installation.cli, Cli::Electron { .. }) {
        cmd.env("ELECTRON_RUN_AS_NODE", "1").env_remove("VSCODE_DEV");
    }
    cmd.current_dir(home).stdin(Stdio::null()).stdout(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Starts the CLI and returns right away. `on_done` gets an error message if it exits non-zero (called
/// on a background thread). Errors starting it are returned directly.
pub fn run(
    installation: &Installation,
    request: &LaunchRequest,
    env: Option<&HashMap<String, String>>,
    home: &Path,
    on_done: impl FnOnce(Option<String>) + Send + 'static,
) -> Result<(), String> {
    // The editor may inherit the CLI's stderr and keep it open for its whole lifetime, so collect it
    // in a temp file instead of a pipe we'd have to drain. Random name, created exclusively and readable
    // only by the user (the temp folder can be shared on Linux); deleted when `log` is dropped.
    let start_error = |e: std::io::Error| format!("Couldn't start the editor: {e}");
    let log = tempfile::Builder::new().prefix("code-toolbox-").suffix(".log").tempfile().map_err(start_error)?;
    let mut cmd = command(installation, request, env, home);
    cmd.stderr(log.reopen().map_err(start_error)?);
    let mut child = cmd.spawn().map_err(start_error)?;
    std::thread::spawn(move || {
        let status = child.wait();
        let text = fs::read_to_string(log.path()).unwrap_or_default();
        drop(log);
        on_done(match status {
            Ok(s) if s.success() => None,
            Ok(s) => Some(format!(
                "The editor exited with {}{}",
                s.code().map_or("a signal".into(), |c| c.to_string()),
                if text.trim().is_empty() { String::new() } else { format!(": {}", text.trim()) }
            )),
            Err(e) => Some(format!("Couldn't wait for the editor: {e}")),
        });
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn arguments() {
        let project = |uri: &str, ws, profile: Option<&str>, nw| LaunchRequest::Project {
            uri: uri.into(),
            is_workspace: ws,
            profile_name: profile.map(str::to_string),
            new_window: nw,
        };
        assert_eq!(project("file:///a", false, None, false).arguments(), ["--folder-uri", "file:///a"]);
        assert_eq!(
            project("file:///a.code-workspace", true, Some("PHP"), true).arguments(),
            ["--profile", "PHP", "--new-window", "--file-uri", "file:///a.code-workspace"]
        );
        assert_eq!(
            project("vscode-remote://ssh-remote%2Bh/x.code-workspace", true, None, false).arguments(),
            ["--file-uri", "vscode-remote://ssh-remote%2Bh/x.code-workspace"]
        );
        assert_eq!(
            LaunchRequest::NewWindow { profile_name: "PHP".into() }.arguments(),
            ["--profile", "PHP", "--new-window"]
        );
        assert_eq!(
            LaunchRequest::File { path: "/x/settings.json".into(), profile_name: Some("PHP".into()) }.arguments(),
            ["--profile", "PHP", "/x/settings.json"]
        );
        assert_eq!(
            LaunchRequest::InstallExtension { vsix: "/r/dev-layout.vsix".into(), profile_name: Some("PHP".into()) }
                .arguments(),
            ["--profile", "PHP", "--install-extension", "/r/dev-layout.vsix", "--force"]
        );
        assert_eq!(
            LaunchRequest::UninstallExtension { id: "old.dev-layout".into(), profile_name: None }.arguments(),
            ["--uninstall-extension", "old.dev-layout"]
        );
    }

    #[test]
    fn windows_runs_electron_directly() {
        let inst = Installation {
            root: PathBuf::from("/vsc"),
            app_dir: PathBuf::from("/vsc/resources/app"),
            cli: Cli::Electron { exe: "/vsc/Code.exe".into(), cli_js: "/vsc/resources/app/out/cli.js".into() },
        };
        let req = LaunchRequest::Project {
            uri: "file:///c%3A/x%20y".into(),
            is_workspace: false,
            profile_name: None,
            new_window: false,
        };
        let cmd = command(&inst, &req, None, Path::new("/"));
        assert_eq!(cmd.get_program(), "/vsc/Code.exe");
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["/vsc/resources/app/out/cli.js", "--folder-uri", "file:///c%3A/x%20y"]);
        assert!(cmd.get_envs().any(|(k, v)| k == "ELECTRON_RUN_AS_NODE" && v == Some("1".as_ref())));
    }

    #[cfg(unix)]
    #[test]
    fn reports_failures() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("code");
        fs::write(&script, "#!/bin/sh\necho 'bad flag' >&2\nexit 3\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let inst = Installation { root: tmp.path().into(), app_dir: tmp.path().into(), cli: Cli::Script(script) };
        let logs = || {
            fs::read_dir(std::env::temp_dir())
                .unwrap()
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("code-toolbox-"))
                .count()
        };
        let before = logs();
        let (tx, rx) = std::sync::mpsc::channel();
        let req = LaunchRequest::NewWindow { profile_name: "X".into() };
        run(&inst, &req, None, tmp.path(), move |e| tx.send(e).unwrap()).unwrap();
        assert_eq!(rx.recv().unwrap().as_deref(), Some("The editor exited with 3: bad flag"));

        let missing = Installation {
            root: tmp.path().into(),
            app_dir: tmp.path().into(),
            cli: Cli::Script(tmp.path().join("nope")),
        };
        assert!(run(&missing, &req, None, tmp.path(), |_| {}).unwrap_err().starts_with("Couldn't start the editor"));
        // No temp logs left behind (the failed launch's included).
        assert_eq!(logs(), before);
    }
}
