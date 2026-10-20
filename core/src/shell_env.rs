//! The environment of an interactive login shell, resolved the way the editor does it when started
//! from the desktop (`$SHELL -i -l -c env`, with its per-shell arguments). The editor skips this when
//! started from its CLI, so the app passes it along; without it, extensions wouldn't find Homebrew,
//! `~/.local/bin` and the like. Not needed on Windows.

use std::collections::HashMap;
use std::ffi::CStr;
use std::io::{ErrorKind, Read};
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const MARKER: &str = "__CODE_TOOLBOX_ENV__";

/// The editor's choice: csh/tcsh only accept `-l` on its own, so they run `-ic` (interactive, not
/// login); PowerShell has its own flags; everything else gets `-i -l -c`.
pub fn arguments(shell: &str, command: &str) -> Vec<String> {
    let name = shell.rsplit('/').next().unwrap_or(shell);
    let flags: &[&str] = match name {
        "csh" | "tcsh" => &["-ic"],
        "pwsh" | "pwsh-preview" | "powershell" | "powershell-preview" => &["-Login", "-Command"],
        _ => &["-i", "-l", "-c"],
    };
    flags.iter().map(|s| s.to_string()).chain([command.to_string()]).collect()
}

/// The user's login shell from the password database, else `$SHELL`.
pub fn login_shell() -> String {
    // SAFETY: getpwuid returns a pointer into static storage (or null); it's read immediately.
    let from_passwd = unsafe {
        let pw = libc::getpwuid(libc::getuid());
        (!pw.is_null() && !(*pw).pw_shell.is_null())
            .then(|| CStr::from_ptr((*pw).pw_shell).to_string_lossy().into_owned())
    };
    from_passwd
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| if cfg!(target_os = "macos") { "/bin/zsh".into() } else { "/bin/sh".into() })
}

/// Runs the shell and returns its environment, or None if it fails, has no PATH, or takes longer than
/// `timeout` (then it's killed and reaped). Output is read without blocking, so a background job that
/// a startup file leaves holding stdout can't keep anything waiting: once the shell has exited, the rest
/// of the output gets at most a second.
pub fn resolve(shell: Option<&str>, timeout: Duration) -> Option<HashMap<String, String>> {
    let shell = shell.map(str::to_string).unwrap_or_else(login_shell);
    let mut cmd = Command::new(&shell);
    cmd.args(arguments(&shell, &format!("printf '\\n{MARKER}\\n'; /usr/bin/env -0")))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Lets shell startup files skip slow or interactive parts, as they can for the editor.
        .env("VSCODE_RESOLVING_ENVIRONMENT", "1");
    if std::env::var_os("TERM").is_none() {
        cmd.env("TERM", "xterm-256color");
    }
    let mut child = cmd.spawn().ok()?;
    let mut out = child.stdout.take()?;
    // SAFETY: plain fcntl on a pipe fd we own.
    unsafe {
        let fd = out.as_raw_fd();
        libc::fcntl(fd, libc::F_SETFL, libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK);
    }

    let deadline = Instant::now() + timeout;
    let mut data = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    let mut eof = false;
    let mut exited: Option<Instant> = None;
    loop {
        let mut progressed = false;
        if !eof {
            match out.read(&mut buf) {
                Ok(0) => eof = true,
                Ok(n) => {
                    data.extend_from_slice(&buf[..n]);
                    progressed = true;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => {}
                Err(_) => eof = true,
            }
        }
        if exited.is_none() && child.try_wait().ok().flatten().is_some() {
            exited = Some(Instant::now());
        }
        match exited {
            Some(at) if eof || at.elapsed() > Duration::from_secs(1) => break,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            _ => {}
        }
        if !progressed {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    drop(out);
    parse(&data)
}

pub fn parse(data: &[u8]) -> Option<HashMap<String, String>> {
    let text = String::from_utf8_lossy(data);
    let start = text.rfind(&format!("\n{MARKER}\n"))? + MARKER.len() + 2;
    let mut env: HashMap<String, String> = text[start..]
        .split('\0')
        .filter_map(|entry| entry.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .filter(|(k, _)| !k.is_empty())
        .collect();
    for key in ["_", "SHLVL", "PWD", "OLDPWD", "VSCODE_RESOLVING_ENVIRONMENT"] {
        env.remove(key);
    }
    env.contains_key("PATH").then_some(env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn parses_env_output() {
        let raw =
            format!("noise from .zshrc\n{MARKER}\nPATH=/opt/homebrew/bin:/usr/bin\0HOME=/Users/me\0SHLVL=2\0X=a=b\0");
        let env = parse(raw.as_bytes()).unwrap();
        assert_eq!(env["PATH"], "/opt/homebrew/bin:/usr/bin");
        assert_eq!(env["X"], "a=b");
        assert!(!env.contains_key("SHLVL"));
    }

    #[test]
    fn shell_arguments_per_shell() {
        assert_eq!(arguments("/bin/zsh", "x"), ["-i", "-l", "-c", "x"]);
        assert_eq!(arguments("/bin/tcsh", "x"), ["-ic", "x"]);
        assert_eq!(arguments("/bin/csh", "x"), ["-ic", "x"]);
        assert_eq!(arguments("/opt/homebrew/bin/pwsh", "x"), ["-Login", "-Command", "x"]);
        assert_eq!(arguments("/usr/bin/fish", "x"), ["-i", "-l", "-c", "x"]);
    }

    #[test]
    fn failure_paths() {
        let tmp = tempfile::tempdir().unwrap();
        // Missing shell: fails right away.
        assert_eq!(resolve(Some(&tmp.path().join("nope").to_string_lossy()), Duration::from_secs(5)), None);

        let script = |name: &str, body: &str| {
            let p = tmp.path().join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p.to_string_lossy().into_owned()
        };
        // Hangs and ignores SIGTERM: killed shortly after the timeout.
        let start = Instant::now();
        assert_eq!(
            resolve(Some(&script("hang", "trap '' TERM\nwhile :; do sleep 1; done\n")), Duration::from_millis(500)),
            None
        );
        assert!(start.elapsed() < Duration::from_secs(4));

        // Prints the environment but leaves a background job holding stdout: still returns promptly.
        let start = Instant::now();
        let bg = script("bg", &format!("sleep 30 &\nprintf '\\n{MARKER}\\n'; /usr/bin/env -0\n"));
        assert!(resolve(Some(&bg), Duration::from_secs(5)).is_some_and(|e| e.contains_key("PATH")));
        assert!(start.elapsed() < Duration::from_secs(4));
    }

    /// The real login shell, plus csh/tcsh where installed (`cargo test -- --ignored`).
    #[test]
    #[ignore]
    fn resolves_real_shells() {
        let env = resolve(None, Duration::from_secs(10)).unwrap();
        println!("PATH={}", env["PATH"]);
        for shell in ["/bin/tcsh", "/bin/csh"].into_iter().filter(|s| std::path::Path::new(s).exists()) {
            let env = resolve(Some(shell), Duration::from_secs(10)).unwrap();
            assert!(env.contains_key("PATH"), "{shell}");
            assert!(!env.contains_key("VSCODE_RESOLVING_ENVIRONMENT"), "{shell}");
        }
    }
}
