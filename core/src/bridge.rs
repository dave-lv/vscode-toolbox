//! Files shared with the dev-layout extension (`vscode-dev-layout/extension.js`):
//!   open/<sha1(uri)>-<pid>.json   written by each editor window while it's open
//!   skip-layout/<sha1(uri)>       written by the app for "Open without dev layout", consumed by the
//!                                 extension (ignored once older than `SKIP_MARKER_LIFETIME`)
//!   close/<sha1(uri)>             written by the app for "Close Window"; the window closes itself

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::platform::Paths;

/// How long a skip-layout marker waits for its window. Keep in sync with SKIP_MARKER_LIFETIME_MS in extension.js.
pub const SKIP_MARKER_LIFETIME: Duration = Duration::from_secs(120);
/// How far the extension host's reported start time may be from the OS's.
const START_TIME_TOLERANCE_MS: i64 = 5_000;

pub fn hash(uri: &str) -> String {
    sha1_smol::Sha1::from(uri).digest().to_string()
}

/// URIs of projects that currently have an editor window. An entry counts only while its extension
/// host is running: the pid must exist and have started when the entry says (a crashed host leaves its
/// file behind, and the OS can hand the pid to another process). Entries that fail this are deleted.
pub fn open_uris(paths: &Paths) -> HashSet<String> {
    let Ok(dir) = fs::read_dir(paths.open_windows_dir()) else { return HashSet::new() };
    let entries: Vec<(PathBuf, String, u32, Option<i64>)> = dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| {
            let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).ok()?).ok()?;
            let uri = json.get("uri")?.as_str()?.to_string();
            let pid = u32::try_from(json.get("pid")?.as_u64()?).ok()?;
            let started = json.get("started").and_then(serde_json::Value::as_f64).map(|ms| ms as i64);
            Some((p, uri, pid, started))
        })
        .collect();

    let starts = start_times_ms(&entries.iter().map(|e| e.2).collect::<Vec<_>>());
    let mut result = HashSet::new();
    for (file, uri, pid, started) in entries {
        match (started, starts.get(&pid)) {
            (Some(s), Some(&actual)) if (actual - s).abs() <= START_TIME_TOLERANCE_MS => {
                result.insert(uri);
            }
            // Gone, reused, or written by an extension version without `started`.
            _ => {
                let _ = fs::remove_file(file);
            }
        }
    }
    result
}

/// Process start times in ms since the epoch, for the pids that exist. The OS reports whole seconds, so
/// the result is accurate to about a second; the tolerance covers that.
///
/// One pid per refresh: with several (duplicates or a pid that no longer exists among them), sysinfo
/// 0.39 returned no processes at all on macOS. There are only a handful of windows.
fn start_times_ms(pids: &[u32]) -> HashMap<u32, i64> {
    let mut unique = pids.to_vec();
    unique.sort_unstable();
    unique.dedup();
    let mut sys = System::new();
    unique
        .into_iter()
        .filter_map(|p| {
            let pid = Pid::from_u32(p);
            sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false, ProcessRefreshKind::nothing());
            sys.process(pid).map(|proc_| (p, proc_.start_time() as i64 * 1000))
        })
        .collect()
}

/// Asks the project's window to close itself (the extension watches `close/`). Only for an open window:
/// the extension deletes markers that already exist when a window opens.
pub fn request_close(uri: &str, paths: &Paths) -> io::Result<()> {
    fs::create_dir_all(paths.close_dir())?;
    fs::write(paths.close_dir().join(hash(uri)), uri)
}

/// Only for a project without an open window: an open window just gets focused, so nothing would
/// consume the marker. The extension ignores markers older than `SKIP_MARKER_LIFETIME` anyway.
pub fn mark_skip_layout(uri: &str, paths: &Paths) -> io::Result<()> {
    fs::create_dir_all(paths.skip_layout_dir())?;
    fs::write(paths.skip_layout_dir().join(hash(uri)), uri)
}

/// Withdraws a marker whose launch failed.
pub fn clear_skip_layout(uri: &str, paths: &Paths) {
    let _ = fs::remove_file(paths.skip_layout_dir().join(hash(uri)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Env, Os, VSCODE};

    #[test]
    fn hash_matches_node() {
        // node -e 'console.log(require("crypto").createHash("sha1").update("file:///a").digest("hex"))'
        assert_eq!(hash("file:///a"), "f8233529d15517953f0e20f3bf44540e732fcc71");
    }

    #[test]
    fn open_windows_need_matching_start_time() {
        let tmp = tempfile::tempdir().unwrap();
        let paths =
            Paths::new(Os::current(), &VSCODE, Env { home: tmp.path().to_string_lossy().into(), ..Env::default() });
        let dir = paths.open_windows_dir();
        fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let started = start_times_ms(&[pid])[&pid];
        let write = |name: &str, json: String| fs::write(dir.join(name), json).unwrap();
        write("live.json", format!(r#"{{"uri":"file:///live","pid":{pid},"started":{}}}"#, started + 300));
        write("reused.json", format!(r#"{{"uri":"file:///reused","pid":{pid},"started":{}}}"#, started - 3_600_000));
        write("old-format.json", format!(r#"{{"uri":"file:///old","pid":{pid}}}"#));
        write("dead.json", format!(r#"{{"uri":"file:///dead","pid":999999,"started":{started}}}"#));

        assert_eq!(open_uris(&paths), HashSet::from(["file:///live".to_string()]));
        let left: Vec<_> =
            fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(left, ["live.json"]);
    }
}
