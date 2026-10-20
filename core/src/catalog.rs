//! The project list as the Projects tab shows it.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::config::Config;
use crate::glob::GlobMatcher;
use crate::models::Project;
use crate::platform::Paths;
use crate::uri::file_uri;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub favorites: Vec<Project>,
    /// By last opened, newest first. Excludes favorites, hidden, excluded and deleted projects.
    pub recent: Vec<Project>,
    /// Found by the folder scan, never opened in the editor.
    pub other: Vec<Project>,
    /// Hidden via "Remove from list" (shown in Settings so they can be restored).
    pub hidden: Vec<Project>,
}

impl Catalog {
    pub fn all(&self) -> impl Iterator<Item = &Project> {
        self.favorites.iter().chain(&self.recent).chain(&self.other)
    }

    pub fn build(projects: &[Project], scanned_paths: &[String], config: &Config, paths: &Paths) -> Catalog {
        let by_uri: HashMap<&str, &Project> = projects.iter().map(|p| (p.uri.as_str(), p)).collect();
        let lookup =
            |uri: &String| by_uri.get(uri.as_str()).map(|p| (*p).clone()).or_else(|| Project::make(uri, paths, None));
        let hidden: HashSet<&str> = config.hidden.iter().map(String::as_str).collect();
        let favorites: HashSet<&str> = config.favorites.iter().map(String::as_str).collect();
        let exclude = GlobMatcher::new(&config.exclude, paths);

        let mut c = Catalog {
            favorites: config.favorites.iter().filter_map(lookup).collect(),
            hidden: config.hidden.iter().filter_map(lookup).collect(),
            ..Catalog::default()
        };

        c.recent = projects
            .iter()
            .filter(|p| {
                if favorites.contains(p.uri.as_str()) || hidden.contains(p.uri.as_str()) {
                    return false;
                }
                if let Some(host) = &p.host_path
                    && (paths.same_path(host, paths.home()) || exclude.matches(host))
                {
                    return false;
                }
                p.exists()
            })
            .cloned()
            .collect();
        c.recent.sort_by_key(|p| std::cmp::Reverse(p.last_opened));

        // Compared by `path_key`: on Windows the editor's paths and the scanned ones can differ in case.
        let known: HashSet<String> = c
            .favorites
            .iter()
            .chain(&c.recent)
            .chain(&c.hidden)
            .filter_map(|p| p.host_path.as_deref().map(|h| paths.path_key(h)))
            .collect();
        c.other = scanned_paths
            .iter()
            .filter(|p| !known.contains(&paths.path_key(p)) && !exclude.matches(p))
            .filter_map(|p| Project::make(&file_uri(p, paths.os), paths, Some(false)))
            .filter(|p| !hidden.contains(p.uri.as_str()))
            .collect();
        c.other.sort_by_key(|p| p.base_name.to_lowercase());
        c
    }
}

/// Search ranking, lower is better: name prefix < name substring < name subsequence < path substring
/// < path subsequence. None = no match.
pub fn fuzzy_score(query: &str, name: &str, detail: &str) -> Option<u8> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    let n = name.to_lowercase();
    let d = detail.to_lowercase();
    if n.starts_with(&q) {
        Some(0)
    } else if n.contains(&q) {
        Some(1)
    } else if is_subsequence(&q, &n) {
        Some(2)
    } else if d.contains(&q) {
        Some(3)
    } else if is_subsequence(&q, &d) {
        Some(4)
    } else {
        None
    }
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut it = haystack.chars();
    needle.chars().filter(|&c| c != ' ').all(|c| it.any(|h| h == c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Env, Os, VSCODE};

    /// On Windows the editor lowercases drive letters; a scan from %USERPROFILE% keeps them uppercase.
    #[test]
    fn windows_scan_doesnt_repeat_known_projects() {
        let paths = Paths::new(Os::Windows, &VSCODE, Env { home: r"C:\Users\Me".into(), ..Env::default() });
        let known = Project::make("file:///c%3A/Users/Me/Projects/shop", &paths, None).unwrap();
        let catalog = Catalog::build(
            &[],
            &[r"C:\Users\Me\Projects\shop".into(), r"C:\Users\Me\Projects\new".into()],
            &Config { favorites: vec![known.uri.clone()], ..Config::default() },
            &paths,
        );
        let other: Vec<_> = catalog.other.iter().map(|p| p.base_name.as_str()).collect();
        assert_eq!(other, ["new"]);
    }

    #[test]
    fn fuzzy() {
        assert_eq!(fuzzy_score("sh", "shop", "~/x"), Some(0));
        assert_eq!(fuzzy_score("hop", "shop", "~/x"), Some(1));
        assert_eq!(fuzzy_score("dass", "daily-assistant", ""), Some(2));
        assert_eq!(fuzzy_score("acme", "shop", "~/Projects/acme/shop"), Some(3));
        assert_eq!(fuzzy_score("ashp", "shop", "~/acme/shop"), Some(4));
        assert_eq!(fuzzy_score("zzz", "shop", "~/x"), None);
        assert_eq!(fuzzy_score("  ", "shop", ""), Some(0));
    }
}
