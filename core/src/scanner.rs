//! Finding project folders the editor has never opened (the "Other" section), and the one-time import
//! of JetBrains Toolbox favorites.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::platform::Paths;

const MARKERS: &[&str] = &[".git", "composer.json", "package.json"];
const SKIPPED: &[&str] = &["node_modules", "vendor", "build", "dist", "target", "Pods"];

/// Folders up to `depth` levels below each root that contain `.git`, `composer.json` or `package.json`.
/// Each folder is listed once, in the order first found, even when roots overlap.
pub fn scan(roots: &[String], depth: usize, paths: &Paths) -> Vec<String> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        // `~/Projects/` and `~/Projects` are the same root.
        let mut dir = paths.expand_tilde(root.trim());
        while dir.len() > 1 && dir.ends_with(['/', '\\']) {
            dir.pop();
        }
        visit(&dir, 1, depth, paths, &mut found, &mut seen);
    }
    found
}

/// Paths are built by joining names (not by resolving them), so they match the editor's URIs exactly.
fn visit(dir: &str, level: usize, depth: usize, paths: &Paths, found: &mut Vec<String>, seen: &mut HashSet<String>) {
    if level > depth {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut names: Vec<String> = entries.flatten().filter_map(|e| e.file_name().to_str().map(str::to_string)).collect();
    names.sort();
    for name in names {
        if name.starts_with('.') || SKIPPED.contains(&name.as_str()) {
            continue;
        }
        let child = paths.join(dir, &[&name]);
        if !Path::new(&child).is_dir() {
            continue;
        }
        if MARKERS.iter().any(|m| Path::new(&child).join(m).exists()) && seen.insert(child.clone()) {
            found.push(child.clone());
        }
        visit(&child, level + 1, depth, paths, found, seen);
    }
}

/// Paths marked as favorite in JetBrains Toolbox (`.settings.json` → `projects[path].favorite`).
pub fn toolbox_favorite_paths(paths: &Paths) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(paths.toolbox_settings()).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut out: Vec<String> = json
        .get("projects")
        .and_then(|p| p.as_object())
        .map(|projects| {
            projects
                .iter()
                .filter(|(_, v)| v.get("favorite").and_then(|f| f.as_bool()) == Some(true))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    Ok(out)
}
