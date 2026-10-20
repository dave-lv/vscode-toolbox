//! Branch and "has uncommitted changes" per project, from `git status --porcelain=v2 --branch`.

use std::io::{Read, Seek};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitInfo {
    pub branch: String,
    pub dirty: bool,
}

/// The repo at `path`, or None if it isn't one (or git is missing or too slow).
///
/// Output goes to an anonymous temp file rather than a pipe: a helper started by git (fsmonitor, a
/// hook) can keep a pipe open after git exits, and reading it would then never finish.
pub fn read(path: &str, timeout: Duration) -> Option<GitInfo> {
    let mut out = tempfile::tempfile().ok()?;
    let mut cmd = Command::new("git");
    cmd.args(["-C", path, "status", "--porcelain=v2", "--branch", "--untracked-files=normal"])
        // Don't take index.lock while the editor or the user is using git.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(out.try_clone().ok()?)
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;
    drop(cmd); // closes our copy of the file handle given to git
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().ok()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if !status.success() {
        return None;
    }
    let mut output = String::new();
    out.rewind().ok()?;
    out.read_to_string(&mut output).ok()?;
    parse(&output)
}

pub fn parse(porcelain_v2: &str) -> Option<GitInfo> {
    let mut branch = None;
    let mut oid = None;
    let mut dirty = false;
    for line in porcelain_v2.lines().filter(|l| !l.is_empty()) {
        if let Some(b) = line.strip_prefix("# branch.head ") {
            branch = Some(b.to_string());
        } else if let Some(o) = line.strip_prefix("# branch.oid ") {
            oid = Some(o.to_string());
        } else if !line.starts_with('#') {
            dirty = true;
        }
    }
    let mut branch = branch?;
    if branch == "(detached)"
        && let Some(oid) = oid
    {
        branch = oid.chars().take(7).collect();
    }
    Some(GitInfo { branch, dirty })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain() {
        let clean = "# branch.oid abc\n# branch.head main\n# branch.upstream origin/main\n";
        assert_eq!(parse(clean), Some(GitInfo { branch: "main".into(), dirty: false }));
        let dirty = "# branch.oid 1234567890\n# branch.head (detached)\n1 .M N... 100644 100644 100644 a b f.txt\n";
        assert_eq!(parse(dirty), Some(GitInfo { branch: "1234567".into(), dirty: true }));
        assert_eq!(parse(""), None);
    }

    #[test]
    fn reads_a_real_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_string_lossy().into_owned();
        let git = |args: &[&str]| Command::new("git").args(["-C", &dir]).args(args).output().unwrap();
        if !git(&["init", "-q", "-b", "trunk"]).status.success() {
            return; // no git here
        }
        assert_eq!(read(&dir, Duration::from_secs(5)), Some(GitInfo { branch: "trunk".into(), dirty: false }));
        std::fs::write(tmp.path().join("f.txt"), "x").unwrap();
        assert_eq!(read(&dir, Duration::from_secs(5)).map(|g| g.dirty), Some(true));
        assert_eq!(read(&tmp.path().join("nope").to_string_lossy(), Duration::from_secs(5)), None);
    }
}
