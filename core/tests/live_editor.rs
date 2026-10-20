//! Opt-in end-to-end check against the real editor: opens a window, waits for the dev-layout extension
//! to register it, asks it to close and waits until it's gone.
//! `CT_TEST_FOLDER=/some/scratch/dir cargo test -p toolbox-core --test live_editor -- --ignored`
//! Uses the PHP profile (where the extension is installed); the folder is opened untrusted, so the
//! layout doesn't start.

use std::path::Path;
use std::time::{Duration, Instant};

use toolbox_core::platform::{Paths, VSCODE};
use toolbox_core::uri::file_uri;
use toolbox_core::{Installation, LaunchRequest, bridge, launcher};

fn wait_until(seconds: u64, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

#[test]
#[ignore]
fn opens_a_window_that_registers_and_closes_on_request() {
    let folder = std::env::var("CT_TEST_FOLDER").expect("set CT_TEST_FOLDER to a scratch folder");
    std::fs::create_dir_all(&folder).unwrap();
    let paths = Paths::current(&VSCODE);
    let installation = Installation::detect(&paths, None).expect("editor not found");
    let uri = file_uri(&folder, paths.os);
    #[cfg(unix)]
    let env = toolbox_core::shell_env::resolve(None, Duration::from_secs(10));
    #[cfg(not(unix))]
    let env = None;

    let request = LaunchRequest::Project {
        uri: uri.clone(),
        is_workspace: false,
        profile_name: Some("PHP".into()),
        new_window: true,
    };
    let (tx, rx) = std::sync::mpsc::channel();
    launcher::run(&installation, &request, env.as_ref(), Path::new(paths.home()), move |e| tx.send(e).unwrap())
        .unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(30)).unwrap(), None, "the CLI failed");

    assert!(wait_until(30, || bridge::open_uris(&paths).contains(&uri)), "window never registered");
    bridge::request_close(&uri, &paths).unwrap();
    assert!(wait_until(15, || !bridge::open_uris(&paths).contains(&uri)), "window didn't close");
}
