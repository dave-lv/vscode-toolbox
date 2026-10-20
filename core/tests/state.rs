//! Reader, catalog, config and scanner against a fake home with editor state shaped like the real files
//! (ported from the Swift app's ToolboxCoreTests).

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use toolbox_core::platform::{Env, Os, Paths, VSCODE};
use toolbox_core::uri::file_uri;
use toolbox_core::{Catalog, Config, ConfigStore, LoadResult, VsCodeReader, scanner};

// The real attached-container authority from the Mac (shop, OrbStack).
const SHOP_CONTAINER_URI: &str = "vscode-remote://attached-container%2B7b22636f6e7461696e65724e616d65223a222f73686f702d73686f702e6465762d31222c2273657474696e6773223a7b22636f6e74657874223a226f7262737461636b227d7d/app";

struct FakeHome {
    /// Kept alive (deleted on drop).
    _dir: tempfile::TempDir,
    paths: Paths,
}

impl FakeHome {
    fn new() -> FakeHome {
        let dir = tempfile::tempdir().unwrap();
        // Real path (macOS /var → /private/var), so tilde() and URIs agree with what's on disk.
        let home = fs::canonicalize(dir.path()).unwrap().to_string_lossy().into_owned();
        let paths = Paths::new(Os::current(), &VSCODE, Env { home, ..Env::default() });
        fs::create_dir_all(paths.workspace_storage()).unwrap();
        fs::create_dir_all(paths.storage_json().parent().unwrap()).unwrap();
        FakeHome { _dir: dir, paths }
    }

    fn root(&self) -> PathBuf {
        PathBuf::from(self.paths.home())
    }

    fn mkdir(&self, rel: &str) -> String {
        let p = self.root().join(rel);
        fs::create_dir_all(&p).unwrap();
        p.to_string_lossy().into_owned()
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.root().join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn add_workspace(&self, hash: &str, json: &str, opened_secs: u64) {
        let dir = self.paths.workspace_storage().join(hash);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("workspace.json"), json).unwrap();
        let db = dir.join("state.vscdb");
        fs::write(&db, "").unwrap();
        let t = filetime::FileTime::from_unix_time(opened_secs as i64, 0);
        filetime::set_file_mtime(&db, t).unwrap();
        filetime::set_file_mtime(&dir, t).unwrap();
    }

    fn reader(&self) -> VsCodeReader<'_> {
        VsCodeReader { paths: &self.paths, installation: None, system_locale: "en-US".into() }
    }
}

fn at(secs: u64) -> Option<SystemTime> {
    Some(UNIX_EPOCH + Duration::from_secs(secs))
}

#[test]
fn reads_projects_profiles_and_associations() {
    let home = FakeHome::new();
    let os = home.paths.os;
    let shop = home.mkdir("Documents/Projects/acme/shop");
    let daily = home.mkdir("Documents/Projects/daily-assistant");
    home.mkdir("Downloads/tet");
    let shop_uri = file_uri(&shop, os);
    let remote_ws = "vscode-remote://ssh-remote%2Bdevbox/home/me/team.code-workspace";

    home.add_workspace("a", &format!(r#"{{"folder":"{shop_uri}"}}"#), 2_000);
    home.add_workspace("b", &format!(r#"{{"folder":"{}"}}"#, file_uri(&daily, os)), 3_000);
    home.add_workspace("c", &format!(r#"{{"folder":"{SHOP_CONTAINER_URI}"}}"#), 1_000);
    home.add_workspace(
        "d",
        &format!(r#"{{"folder":"{}"}}"#, file_uri(&format!("{}/Downloads/tet", home.paths.home()), os)),
        4_000,
    );
    home.add_workspace(
        "e",
        &format!(r#"{{"folder":"{}"}}"#, file_uri(&format!("{}/gone", home.paths.home()), os)),
        5_000,
    );
    home.add_workspace("f", "{}", 6_000);
    home.add_workspace("g", &format!(r#"{{"workspace":"{remote_ws}"}}"#), 1_500);

    let storage = home.paths.storage_json();
    fs::write(
        &storage,
        format!(
            r#"{{
              "userDataProfiles": [
                {{"location": "4a1b9c7e", "name": "PHP"}},
                {{"location": "builtin/agents", "name": "Agents", "icon": "robot",
                 "useDefaultFlags": {{"extensions": true, "settings": true, "keybindings": true}}}}
              ],
              "profileAssociations": {{"workspaces": {{
                "{shop_uri}": "4a1b9c7e",
                "{SHOP_CONTAINER_URI}": "4a1b9c7e",
                "{}": "__default__profile__"
              }}}}
            }}"#,
            file_uri(&daily, os)
        ),
    )
    .unwrap();
    home.write(
        ".vscode/extensions/extensions.json",
        r#"[{"identifier":{"id":"anthropic.claude-code"},"version":"2.1.3","location":{"$mid":1,"path":"/nope"}}]"#,
    );
    let php_ext = home.paths.code_user().join("profiles/4a1b9c7e/extensions.json");
    fs::create_dir_all(php_ext.parent().unwrap()).unwrap();
    fs::write(
        php_ext,
        r#"[{"identifier":{"id":"bmewburn.vscode-intelephense-client"},"version":"1.14"},{"identifier":{"id":"xdebug.php-debug"},"version":"1.3"}]"#,
    )
    .unwrap();

    let snap = home.reader().read();
    assert_eq!(snap.projects.len(), 6);
    let find = |uri: &str| snap.projects.iter().find(|p| p.uri == uri).unwrap();
    assert_eq!(find(&shop_uri).profile_id.as_deref(), Some("4a1b9c7e"));
    assert_eq!(find(&shop_uri).last_opened, at(2_000));
    assert!(find(remote_ws).is_workspace);
    assert!(!find(SHOP_CONTAINER_URI).is_workspace);

    let names: Vec<_> = snap.profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Default", "PHP", "Agents"]);
    let cli: Vec<_> = snap.profiles.iter().map(|p| p.cli_name.as_str()).collect();
    assert_eq!(cli, ["Default", "PHP", "Agents"]);
    let ids: Vec<_> = snap.profiles[1].extensions.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["bmewburn.vscode-intelephense-client", "xdebug.php-debug"]);
    assert!(snap.profiles[2].is_builtin);
    assert_eq!(snap.profiles[2].icon.as_deref(), Some("robot"));
    assert_eq!(snap.profiles[2].extensions[0].id, "anthropic.claude-code");
    // Agents shares the Default profile's settings and keybindings; PHP has its own.
    let user = home.paths.code_user();
    assert_eq!(snap.profiles[2].settings_file, user.join("settings.json"));
    assert_eq!(snap.profiles[2].keybindings_file, user.join("keybindings.json"));
    assert_eq!(snap.profiles[1].settings_file, user.join("profiles/4a1b9c7e/settings.json"));

    let config =
        Config { favorites: vec![file_uri(&daily, os), file_uri("/Users/elsewhere/billing", os)], ..Config::default() };
    let catalog = Catalog::build(&snap.projects, &[], &config, &home.paths);
    let favs: Vec<_> = catalog.favorites.iter().map(|p| p.base_name.as_str()).collect();
    assert_eq!(favs, ["daily-assistant", "billing"]);
    // Downloads excluded by default, "gone" no longer exists, favorites not repeated; newest first.
    let recent: Vec<_> = catalog.recent.iter().map(|p| p.uri.as_str()).collect();
    assert_eq!(recent, [shop_uri.as_str(), remote_ws, SHOP_CONTAINER_URI]);
}

/// Older editor versions also keep a recent list in state.vscdb; entries missing from workspaceStorage
/// come from there (without a date).
#[cfg(feature = "recent-db")]
#[test]
fn reads_the_recent_list_from_state_db() {
    let home = FakeHome::new();
    let folder = home.mkdir("Projects/old");
    let db = rusqlite::Connection::open(home.paths.global_state_db()).unwrap();
    db.execute("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)", []).unwrap();
    let list = serde_json::json!({"entries": [
        {"folderUri": file_uri(&folder, home.paths.os)},
        {"workspace": {"id": "x", "configPath": "file:///w/team.code-workspace"}},
        {"fileUri": "file:///some/file.txt"}
    ]});
    db.execute("INSERT INTO ItemTable VALUES ('history.recentlyOpenedPathsList', ?1)", [list.to_string()]).unwrap();
    drop(db);

    let snap = home.reader().read();
    let mut found: Vec<_> =
        snap.projects.iter().map(|p| (p.base_name.as_str(), p.is_workspace, p.last_opened)).collect();
    found.sort_by_key(|f| f.0);
    assert_eq!(found, [("old", false, None), ("team", true, None)]);
}

#[test]
fn default_profile_name_follows_the_editors_language() {
    let home = FakeHome::new();
    let name = |locale: &str| {
        VsCodeReader { paths: &home.paths, installation: None, system_locale: locale.into() }.default_profile_name()
    };
    // No language packs: always English.
    assert_eq!(name("de-DE"), "Default");

    let translations = home.root().join("de-pack/main.i18n.json");
    home.write(
        "de-pack/main.i18n.json",
        r#"{"contents": {"vs/platform/userDataProfile/common/userDataProfile": {"defaultProfile": "Standard"}}}"#,
    );
    let packs = home.paths.language_packs_json();
    fs::create_dir_all(packs.parent().unwrap()).unwrap();
    fs::write(packs, serde_json::json!({"de": {"hash": "x", "translations": {"vscode": translations}}}).to_string())
        .unwrap();
    assert_eq!(name("de-DE"), "Standard"); // de-de falls back to the de pack
    assert_eq!(name("en-US"), "Default");
    assert_eq!(name("lv-LV"), "Default"); // no pack for lv

    // argv.json's locale (JSON with comments) wins over the OS language.
    home.write(
        ".vscode/argv.json",
        "// comment\n{\n    // \"locale\": \"lv\",\n    \"locale\": \"en\", /* trailing comma next */\n}\n",
    );
    assert_eq!(name("de-DE"), "Default");
}

#[test]
fn scanner_and_other_section() {
    let home = FakeHome::new();
    home.mkdir("Documents/Projects/a/.git");
    home.write("Documents/Projects/group/b/composer.json", "{}");
    home.write("Documents/Projects/group/b/vendor/c/package.json", "{}");
    home.write("Documents/Projects/x/y/z/package.json", "{}");
    let found = scanner::scan(&["~/Documents/Projects".into()], 2, &home.paths);
    let mut tilded: Vec<_> = found.iter().map(|p| home.paths.tilde(p)).collect();
    tilded.sort();
    assert_eq!(tilded, ["~/Documents/Projects/a", "~/Documents/Projects/group/b"]);

    // Overlapping roots (and a trailing slash) list each project once.
    let overlapping = scanner::scan(
        &["~/Documents/Projects".into(), "~/Documents/Projects/group/".into(), "~/Documents/Projects".into()],
        2,
        &home.paths,
    );
    assert_eq!(overlapping, found);

    // Scanned folders the editor never opened become "Other", by name.
    let catalog = Catalog::build(&[], &found, &Config::default(), &home.paths);
    let other: Vec<_> = catalog.other.iter().map(|p| p.base_name.as_str()).collect();
    assert_eq!(other, ["a", "b"]);
}

#[test]
fn save_if_unchanged_refuses_to_overwrite_newer_edits() {
    let home = FakeHome::new();
    let c = Config { favorites: vec!["file:///mine".into()], ..Config::default() };
    // Missing file: expected None, so it saves.
    assert!(ConfigStore::save_if_unchanged(&c, &home.paths, None).unwrap());
    let (_, base) = ConfigStore::load_with_bytes(&home.paths);
    // Edited by hand meanwhile: refused, and the edit survives.
    fs::write(home.paths.config_file(), r#"{"favorites":["file:///theirs"]}"#).unwrap();
    assert!(!ConfigStore::save_if_unchanged(&c, &home.paths, base.as_deref()).unwrap());
    assert!(fs::read_to_string(home.paths.config_file()).unwrap().contains("theirs"));
    // Against the current bytes it saves.
    let (_, base) = ConfigStore::load_with_bytes(&home.paths);
    assert!(ConfigStore::save_if_unchanged(&c, &home.paths, base.as_deref()).unwrap());
}

#[test]
fn config_missing_versus_unreadable() {
    let home = FakeHome::new();
    assert_eq!(ConfigStore::load(&home.paths), LoadResult::Missing);

    // A directory where the file should be: exists but can't be read, so it isn't "missing".
    fs::create_dir_all(home.paths.config_file()).unwrap();
    assert!(matches!(ConfigStore::load(&home.paths), LoadResult::Invalid(_)));
    fs::remove_dir(home.paths.config_file()).unwrap();

    #[cfg(unix)]
    {
        // A dangling symlink isn't missing either.
        std::os::unix::fs::symlink("/nonexistent/x", home.paths.config_file()).unwrap();
        assert!(matches!(ConfigStore::load(&home.paths), LoadResult::Invalid(_)));
    }
}

#[cfg(unix)]
#[test]
fn save_keeps_symlink_and_reset_moves_broken_file_aside() {
    let home = FakeHome::new();
    // config.json → ../../dotfiles/toolbox.json (relative link), like a work-env symlink.
    let real = home.root().join("dotfiles/toolbox.json");
    home.write("dotfiles/toolbox.json", r#"{"favorites":["file:///a"]}"#);
    fs::create_dir_all(home.paths.config_dir()).unwrap();
    std::os::unix::fs::symlink("../../dotfiles/toolbox.json", home.paths.config_file()).unwrap();
    assert_eq!(ConfigStore::target(&home.paths), real);

    let c = Config { favorites: vec!["file:///b".into()], ..Config::default() };
    ConfigStore::save(&c, &home.paths).unwrap();
    assert!(fs::read_link(home.paths.config_file()).is_ok(), "link was replaced");
    let LoadResult::Loaded(saved) = ConfigStore::load(&home.paths) else { panic!("not loaded") };
    assert_eq!(saved.favorites, ["file:///b"]);
    assert!(fs::read_to_string(&real).unwrap().ends_with("}\n"));

    // Broken target: reset moves it aside next to the target and writes a new one; the link stays.
    fs::write(&real, "{ not json").unwrap();
    assert!(matches!(ConfigStore::load(&home.paths), LoadResult::Invalid(_)));
    let backup = ConfigStore::replace_broken(&c, &home.paths).unwrap().unwrap();
    assert_eq!(backup.file_name().unwrap(), "toolbox.json.broken");
    assert_eq!(fs::read_to_string(&backup).unwrap(), "{ not json");
    assert!(fs::read_link(home.paths.config_file()).is_ok());
    assert!(matches!(ConfigStore::load(&home.paths), LoadResult::Loaded(_)));
}

/// Reads the real config.json with the Rust code and writes it back to a temp file: the bytes must be
/// identical to what the Swift app wrote (`cargo test -- --ignored`).
#[test]
#[ignore]
fn round_trips_the_real_config_byte_for_byte() {
    let paths = Paths::current(&VSCODE);
    let original = fs::read_to_string(paths.config_file()).unwrap();
    let LoadResult::Loaded(config) = ConfigStore::load(&paths) else { panic!("real config didn't load") };
    let mut text = toolbox_core::config::to_apple_json(&serde_json::to_value(&config).unwrap());
    text.push('\n');
    assert_eq!(text, original);
}
