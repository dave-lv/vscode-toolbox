//! VS Code's "JSON with comments": removes `//` and `/* */` comments and trailing commas so serde_json
//! can read it. Text inside strings is left alone.

pub fn strip(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_string = false;

    // Index just past the comment starting at `j` (`//` or `/*`), or None if there's none.
    let skip_comment = |j: usize| -> Option<usize> {
        match (chars.get(j), chars.get(j + 1)) {
            (Some('/'), Some('/')) => Some(chars[j..].iter().position(|&c| c == '\n').map_or(chars.len(), |n| j + n)),
            (Some('/'), Some('*')) => {
                let mut k = j + 2;
                while k < chars.len() && !(chars[k] == '*' && chars.get(k + 1) == Some(&'/')) {
                    k += 1;
                }
                Some((k + 2).min(chars.len()))
            }
            _ => None,
        }
    };

    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\'
                && let Some(&next) = chars.get(i + 1)
            {
                out.push(next);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
        } else if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
        } else if let Some(end) = skip_comment(i) {
            i = end;
        } else if c == ',' {
            // Drop it when only whitespace/comments separate it from a closing bracket.
            let mut j = i + 1;
            loop {
                if j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                } else if let Some(end) = skip_comment(j) {
                    j = end;
                } else {
                    break;
                }
            }
            if !matches!(chars.get(j), None | Some('}') | Some(']')) {
                out.push(c);
            }
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Parses JSON with comments into an object, or None.
pub fn parse_object(text: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    serde_json::from_str(&strip(text)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc() {
        let text =
            "// header\n{ \"a\": \"x // not a comment\", /* block */ \"b\": [1, 2,], \"c\": \"\\\"quoted\\\" /*\", }";
        let obj = parse_object(text).unwrap();
        assert_eq!(obj["a"], "x // not a comment");
        assert_eq!(obj["b"], serde_json::json!([1, 2]));
        assert_eq!(obj["c"], "\"quoted\" /*");
    }
}
