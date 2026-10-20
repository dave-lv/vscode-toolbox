//! Code Toolbox core: everything except the UI and the tray. Reads VS Code's state, builds the
//! project list, keeps `config.json`, starts the editor's CLI and talks to the dev-layout extension
//! through marker files. Ported from the original Swift app's ToolboxCore (see PLAN-cross-platform.md).
//!
//! No async and no threads of its own, except where a child process must be waited on.

pub mod bridge;
pub mod catalog;
pub mod config;
pub mod git;
pub mod glob;
pub mod install;
pub mod jsonc;
pub mod launcher;
pub mod models;
pub mod platform;
pub mod reader;
pub mod scanner;
#[cfg(unix)]
pub mod shell_env;
pub mod uri;

pub use catalog::{Catalog, fuzzy_score};
pub use config::{Config, ConfigStore, LoadResult, ScanConfig};
pub use git::GitInfo;
pub use install::{Cli, Installation};
pub use launcher::LaunchRequest;
pub use models::{ExtensionInfo, Profile, Project, ProjectKind};
pub use platform::{EDITORS, Editor, Env, Os, Paths};
pub use reader::{Snapshot, VsCodeReader};
pub use uri::{RemoteTarget, VsUri};
