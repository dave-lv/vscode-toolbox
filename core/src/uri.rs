//! VS Code resource URIs as stored in its state files, e.g. `file:///Users/me/Projects/shop`,
//! `file:///c%3A/Users/me/x` or `vscode-remote://attached-container%2B7b22…/app`.

use crate::platform::Os;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VsUri {
    pub raw: String,
    pub scheme: String,
    /// Percent-decoded, e.g. `attached-container+7b22…`.
    pub authority: String,
    /// Percent-decoded, e.g. `/Users/me/My Project` or `/c:/Users/me`.
    pub path: String,
}

impl VsUri {
    pub fn parse(raw: &str) -> Option<VsUri> {
        let sep = raw.find("://")?;
        let scheme = raw[..sep].to_ascii_lowercase();
        let mut rest = &raw[sep + 3..];
        if let Some(cut) = rest.find(['?', '#']) {
            rest = &rest[..cut];
        }
        let slash = rest.find('/').unwrap_or(rest.len());
        Some(VsUri {
            raw: raw.to_string(),
            scheme,
            authority: percent_decode(&rest[..slash]),
            path: percent_decode(&rest[slash..]),
        })
    }

    /// The path on this machine for a `file:` URI, like VS Code's `URI.fsPath`.
    pub fn fs_path(&self, os: Os) -> String {
        if os == Os::Windows && self.scheme == "file" && !self.authority.is_empty() && self.path.len() > 1 {
            return format!(r"\\{}{}", self.authority, self.path.replace('/', "\\"));
        }
        fs_path_from_uri_path(&self.path, os)
    }
}

/// `/c:/Users/me` → `c:\Users\me` on Windows (VS Code lowercases the drive letter); unchanged elsewhere.
pub fn fs_path_from_uri_path(path: &str, os: Os) -> String {
    if os != Os::Windows {
        return path.to_string();
    }
    let b = path.as_bytes();
    let p = if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        format!("{}{}", (b[1] as char).to_ascii_lowercase(), &path[2..])
    } else {
        path.to_string()
    };
    p.replace('/', "\\")
}

/// Same string as VS Code's `URI.file(path).toString()`: unreserved characters and `/` stay, everything
/// else is percent-encoded. On Windows: backslashes become `/`, the drive letter is lowercased and its
/// `:` encoded (`file:///c%3A/Users/x`), and UNC paths put the server in the authority.
pub fn file_uri(path: &str, os: Os) -> String {
    if os != Os::Windows {
        return format!("file://{}", encode(path));
    }
    let p = path.replace('\\', "/");
    if let Some(unc) = p.strip_prefix("//") {
        let (server, rest) = unc.split_once('/').unwrap_or((unc, ""));
        return format!("file://{}{}", encode(&server.to_lowercase()), encode(&format!("/{rest}")));
    }
    let b = p.as_bytes();
    let p = if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        format!("/{}{}", (b[0] as char).to_ascii_lowercase(), &p[1..])
    } else {
        p
    };
    format!("file://{}", encode(&p))
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &byte in s.as_bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Percent-decodes; returns the input unchanged if the result isn't valid UTF-8.
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let (Some(h), Some(l)) = (hex(b.get(i + 1)), hex(b.get(i + 2)))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

fn hex(c: Option<&u8>) -> Option<u8> {
    match *c? {
        c @ b'0'..=b'9' => Some(c - b'0'),
        c @ b'a'..=b'f' => Some(c - b'a' + 10),
        c @ b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// What a `vscode-remote://<type>+<value>` authority points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteTarget {
    /// "Attach to running container": value is hex-encoded JSON `{"containerName":"/shop-shop.dev-1",…}`.
    AttachedContainer {
        container_name: String,
    },
    /// "Reopen in container": value is hex-encoded JSON with `hostPath`, or (older) the hex-encoded host path.
    DevContainer {
        host_path: Option<String>,
    },
    Ssh {
        host: String,
    },
    Wsl {
        distro: String,
    },
    Other {
        kind: String,
    },
}

impl RemoteTarget {
    pub fn parse(authority: &str) -> RemoteTarget {
        let (kind, value) = authority.split_once('+').unwrap_or((authority, ""));
        let decoded = hex_decode(value);
        let json: Option<serde_json::Map<String, serde_json::Value>> =
            decoded.as_deref().and_then(|d| serde_json::from_str(d).ok());
        let field = |k: &str| json.as_ref().and_then(|j| j.get(k)).and_then(|v| v.as_str()).map(str::to_string);

        match kind {
            "attached-container" => {
                let name = field("containerName").or_else(|| decoded.clone()).unwrap_or_else(|| value.to_string());
                RemoteTarget::AttachedContainer { container_name: name.strip_prefix('/').unwrap_or(&name).to_string() }
            }
            "dev-container" => match &json {
                Some(_) => RemoteTarget::DevContainer { host_path: field("hostPath") },
                None => RemoteTarget::DevContainer { host_path: decoded },
            },
            "ssh-remote" => RemoteTarget::Ssh { host: field("hostName").unwrap_or_else(|| value.to_string()) },
            "wsl" => RemoteTarget::Wsl { distro: value.to_string() },
            _ => RemoteTarget::Other { kind: kind.to_string() },
        }
    }
}

fn hex_decode(s: &str) -> Option<String> {
    if s.is_empty() || !s.len().is_multiple_of(2) || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect();
    String::from_utf8(bytes?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The real attached-container authority from the Mac (shop, OrbStack).
    pub const SHOP_CONTAINER_URI: &str = "vscode-remote://attached-container%2B7b22636f6e7461696e65724e616d65223a222f73686f702d73686f702e6465762d31222c2273657474696e6773223a7b22636f6e74657874223a226f7262737461636b227d7d/app";

    #[test]
    fn file_uri_encoding_matches_vscode() {
        assert_eq!(file_uri("/Users/me/Projects/shop", Os::Macos), "file:///Users/me/Projects/shop");
        assert_eq!(
            file_uri("/Users/me/50_99 Premium Website(1)", Os::Macos),
            "file:///Users/me/50_99%20Premium%20Website%281%29"
        );
        assert_eq!(file_uri("/Users/me/rīga", Os::Linux), "file:///Users/me/r%C4%ABga");
    }

    #[test]
    fn windows_file_uris() {
        assert_eq!(file_uri(r"C:\Users\Me\x y", Os::Windows), "file:///c%3A/Users/Me/x%20y");
        assert_eq!(file_uri(r"\\Server\share\x", Os::Windows), "file://server/share/x");
        let u = VsUri::parse("file:///c%3A/Users/Me/x%20y").unwrap();
        assert_eq!(u.path, "/c:/Users/Me/x y");
        assert_eq!(u.fs_path(Os::Windows), r"c:\Users\Me\x y");
        assert_eq!(VsUri::parse("file://server/share/x").unwrap().fs_path(Os::Windows), r"\\server\share\x");
        // Round trip.
        assert_eq!(file_uri(&u.fs_path(Os::Windows), Os::Windows), "file:///c%3A/Users/Me/x%20y");
    }

    #[test]
    fn parses_file_uri() {
        let u = VsUri::parse("file:///Users/me/50_99%20Premium%20Website%281%29").unwrap();
        assert_eq!(u.scheme, "file");
        assert_eq!(u.authority, "");
        assert_eq!(u.path, "/Users/me/50_99 Premium Website(1)");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
    }

    #[test]
    fn decodes_attached_container() {
        let u = VsUri::parse(SHOP_CONTAINER_URI).unwrap();
        assert_eq!(u.path, "/app");
        assert_eq!(
            RemoteTarget::parse(&u.authority),
            RemoteTarget::AttachedContainer { container_name: "shop-shop.dev-1".into() }
        );
    }

    #[test]
    fn decodes_dev_container_both_formats() {
        let hex = |s: &str| s.bytes().map(|b| format!("{b:02x}")).collect::<String>();
        let json = r#"{"hostPath":"/Users/me/globex/billing","configFile":{"$mid":1,"path":"/x"}}"#;
        assert_eq!(
            RemoteTarget::parse(&format!("dev-container+{}", hex(json))),
            RemoteTarget::DevContainer { host_path: Some("/Users/me/globex/billing".into()) }
        );
        assert_eq!(
            RemoteTarget::parse(&format!("dev-container+{}", hex("/Users/me/globex/billing"))),
            RemoteTarget::DevContainer { host_path: Some("/Users/me/globex/billing".into()) }
        );
        assert_eq!(RemoteTarget::parse("ssh-remote+devbox"), RemoteTarget::Ssh { host: "devbox".into() });
        assert_eq!(RemoteTarget::parse("wsl+Ubuntu"), RemoteTarget::Wsl { distro: "Ubuntu".into() });
    }
}
