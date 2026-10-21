//! Updates from the public GitHub releases. Each release has a `latest.json` naming every platform's
//! signed installer (written by .github/workflows/build.yml); the updater plugin checks the signature
//! against the public key in tauri.conf.json before installing anything. Checked a while after startup
//! and then every few hours, unless turned off in Settings.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::model::{self, show_banner, with_model};

/// Where the panel sends people whose install can't update itself.
pub const RELEASES_URL: &str = "https://github.com/dave-lv/vscode-toolbox/releases/latest";
const FIRST_CHECK: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// What the panel shows about a newer version.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    /// This install can replace itself. False for the Arch package (repackaged from the .deb, which the
    /// updater would try to install with dpkg): the panel links to the release instead.
    pub can_install: bool,
    /// Download progress in percent, once installing has started.
    pub progress: Option<u8>,
}

/// The update found by the last check, kept for installing it.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            if with_model(&app, |m| m.config.check_updates) {
                tauri::async_runtime::block_on(check(&app, false));
            }
            std::thread::sleep(CHECK_EVERY);
        }
    });
}

/// `manual`: from Settings, so "no update" and errors are reported too.
pub async fn check(app: &AppHandle, manual: bool) {
    let result = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    match result {
        Ok(Some(update)) => {
            let info = UpdateInfo { version: update.version.clone(), can_install: can_install(), progress: None };
            *app.state::<Pending>().0.lock().unwrap() = Some(update);
            with_model(app, |m| {
                // A version dismissed with ✕ stays hidden until the app restarts; a newer one shows again.
                if m.dismissed_update.as_deref() != Some(info.version.as_str()) || manual {
                    m.dismissed_update = None;
                    m.update = Some(info);
                }
            });
            model::emit_state(app);
        }
        Ok(None) if manual => {
            show_banner(app, format!("Code Toolbox {} is the latest version", app.package_info().version), false)
        }
        Err(e) if manual => show_banner(app, format!("Couldn't check for updates: {e}"), true),
        _ => {}
    }
}

/// Downloads and installs the pending update, then restarts into it (on Windows the installer closes the
/// app itself). Where the install can't update itself, opens the release page instead.
pub async fn install(app: &AppHandle) {
    let Some(update) = app.state::<Pending>().0.lock().unwrap().clone() else { return };
    if !can_install() {
        if let Err(e) = app.opener().open_url(RELEASES_URL, None::<&str>) {
            show_banner(app, format!("Couldn't open {RELEASES_URL}: {e}"), true);
        }
        return;
    }
    set_progress(app, Some(0));
    let mut received = 0u64;
    let result = update
        .download_and_install(
            |chunk, total| {
                received += chunk as u64;
                if let Some(total) = total.filter(|t| *t > 0) {
                    set_progress(app, Some((received * 100 / total).min(100) as u8));
                }
            },
            || {},
        )
        .await;
    match result {
        Ok(()) => app.restart(),
        Err(e) => {
            set_progress(app, None);
            show_banner(app, format!("Couldn't install the update: {e}"), true);
        }
    }
}

pub fn dismiss(app: &AppHandle) {
    with_model(app, |m| {
        m.dismissed_update = m.update.take().map(|u| u.version);
    });
    model::emit_state(app);
}

fn set_progress(app: &AppHandle, progress: Option<u8>) {
    let changed = with_model(app, |m| match m.update.as_mut() {
        Some(u) if u.progress != progress => {
            u.progress = progress;
            true
        }
        _ => false,
    });
    if changed {
        model::emit_state(app);
    }
}

/// macOS and Windows replace the app in place; Linux installs the .deb or .rpm it came from (asking for the
/// admin password), except the Arch package, which is the .deb's files under pacman.
fn can_install() -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::path::Path;
        use tauri::utils::{config::BundleType, platform::bundle_type};
        let pacman = Path::new("/usr/bin/pacman").exists();
        match bundle_type() {
            Some(BundleType::Deb) => !pacman && Path::new("/usr/bin/dpkg").exists(),
            Some(BundleType::Rpm) => !pacman && Path::new("/usr/bin/rpm").exists(),
            _ => false,
        }
    }
    #[cfg(not(target_os = "linux"))]
    true
}
