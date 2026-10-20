//! Minimal glob matching for the `exclude` setting: `**/` is zero or more folders, `**` anything,
//! `*` and `?` stay within one folder. Patterns may start with `~`. On Windows, matching ignores case
//! and separator style.

use crate::platform::{Os, Paths};

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Lit(char),
    /// `**/`
    AnyDirs,
    /// `**`
    AnyAll,
    /// `*`
    Star,
    /// `?`
    Quest,
}

#[derive(Debug, Clone)]
pub struct GlobMatcher {
    patterns: Vec<Vec<Token>>,
    windows: bool,
}

impl GlobMatcher {
    pub fn new(patterns: &[String], paths: &Paths) -> GlobMatcher {
        let windows = paths.os == Os::Windows;
        let patterns = patterns
            .iter()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .map(|p| tokenize(&normalize(&paths.expand_tilde(p), windows)))
            .collect();
        GlobMatcher { patterns, windows }
    }

    pub fn matches(&self, path: &str) -> bool {
        let path: Vec<char> = normalize(path, self.windows).chars().collect();
        self.patterns.iter().any(|p| matches_at(p, &path))
    }
}

fn normalize(s: &str, windows: bool) -> String {
    if windows { s.replace('\\', "/").to_lowercase() } else { s.to_string() }
}

fn tokenize(glob: &str) -> Vec<Token> {
    let chars: Vec<char> = glob.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                if chars.get(i + 2) == Some(&'/') {
                    out.push(Token::AnyDirs);
                    i += 3;
                } else {
                    out.push(Token::AnyAll);
                    i += 2;
                }
            }
            '*' => {
                out.push(Token::Star);
                i += 1;
            }
            '?' => {
                out.push(Token::Quest);
                i += 1;
            }
            c => {
                out.push(Token::Lit(c));
                i += 1;
            }
        }
    }
    out
}

/// Whole-string match with backtracking (patterns and paths are short).
fn matches_at(tokens: &[Token], s: &[char]) -> bool {
    let Some((first, rest)) = tokens.split_first() else { return s.is_empty() };
    match first {
        Token::Lit(c) => s.first() == Some(c) && matches_at(rest, &s[1..]),
        Token::Quest => s.first().is_some_and(|&c| c != '/') && matches_at(rest, &s[1..]),
        Token::Star => {
            for i in 0..=s.len() {
                if matches_at(rest, &s[i..]) {
                    return true;
                }
                if i < s.len() && s[i] == '/' {
                    return false;
                }
            }
            false
        }
        Token::AnyAll => (0..=s.len()).any(|i| matches_at(rest, &s[i..])),
        // Nothing, or anything that ends with a `/`.
        Token::AnyDirs => matches_at(rest, s) || (1..=s.len()).any(|i| s[i - 1] == '/' && matches_at(rest, &s[i..])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Env, VSCODE};

    fn matcher(os: Os, home: &str, patterns: &[&str]) -> GlobMatcher {
        let paths = Paths::new(os, &VSCODE, Env { home: home.into(), ..Env::default() });
        GlobMatcher::new(&patterns.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &paths)
    }

    #[test]
    fn globs() {
        let m = matcher(Os::Macos, "/Users/me", &["~/Downloads/**", "**/trash", "/tmp/*.x"]);
        assert!(m.matches("/Users/me/Downloads/tet"));
        assert!(m.matches("/Users/me/Downloads/a/b"));
        assert!(!m.matches("/Users/me/Documents/tet"));
        assert!(m.matches("/Users/me/Documents/Projects/trash"));
        assert!(!m.matches("/Users/me/Documents/Projects/trashcan"));
        assert!(m.matches("/tmp/a.x"));
        assert!(!m.matches("/tmp/a/b.x"));
        let q = matcher(Os::Linux, "/home/me", &["/srv/?b"]);
        assert!(q.matches("/srv/ab"));
        assert!(!q.matches("/srv//b"));
    }

    #[test]
    fn windows_globs_ignore_case_and_separators() {
        let m = matcher(Os::Windows, r"C:\Users\Me", &["~/Downloads/**"]);
        assert!(m.matches(r"c:\users\me\downloads\x"));
        assert!(!m.matches(r"c:\users\me\documents\x"));
    }
}
