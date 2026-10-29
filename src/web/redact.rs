use std::cmp::Reverse;
use std::path::PathBuf;

/// Replaces server paths in a message sent to clients, longest path first. A path
/// only matches at the start of a path in the message, and up to a separator or its
/// end, so `/srv/data` leaves `/srv/database` and `s3://bucket/srv/data` alone. An
/// empty replacement makes the paths below it relative, and the path itself `.`.
pub(crate) fn redact_paths(message: &str, paths: &[(PathBuf, String)]) -> String {
    let mut paths: Vec<(&str, &str)> = paths
        .iter()
        .filter_map(|(path, replacement)| {
            let path = path.to_str()?.trim_end_matches(is_separator);
            (!path.is_empty()).then_some((path, replacement.as_str()))
        })
        .collect();
    paths.sort_by_key(|(path, _)| Reverse(path.len()));

    let mut redacted = String::with_capacity(message.len());
    let mut rest = message;
    let mut in_path = false;
    while let Some(c) = rest.chars().next() {
        let matched = (!in_path)
            .then(|| {
                paths
                    .iter()
                    .find_map(|(path, rep)| match_path(rest, path, rep))
            })
            .flatten();
        if let Some((consumed, replacement)) = matched {
            redacted.push_str(replacement);
            rest = &rest[consumed..];
            in_path = true;
        } else {
            redacted.push(c);
            rest = &rest[c.len_utf8()..];
            in_path = is_path_char(c);
        }
    }
    redacted
}

/// If `text` starts with `path`, returns the length to replace and its replacement.
fn match_path<'a>(text: &str, path: &str, replacement: &'a str) -> Option<(usize, &'a str)> {
    let after = text.strip_prefix(path)?;
    let whole = if replacement.is_empty() {
        "."
    } else {
        replacement
    };
    let mut chars = after.chars();
    match chars.next() {
        None => Some((path.len(), whole)),
        Some(c) if is_separator(c) => match chars.next() {
            // The path itself, written with a trailing separator.
            None => Some((path.len() + 1, whole)),
            Some(next) if !is_path_char(next) => Some((path.len() + 1, whole)),
            // Below the path: drop the separator to make it relative.
            Some(_) if replacement.is_empty() => Some((path.len() + 1, "")),
            Some(_) => Some((path.len(), replacement)),
        },
        Some(c) if !is_path_char(c) => Some((path.len(), whole)),
        Some(_) => None,
    }
}

fn is_separator(c: char) -> bool {
    c == '/' || c == '\\'
}

fn is_path_char(c: char) -> bool {
    !(c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '(' | ')' | '[' | ']' | ',' | ';'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redact(message: &str, paths: &[(&str, &str)]) -> String {
        let paths: Vec<(PathBuf, String)> = paths
            .iter()
            .map(|(path, replacement)| (PathBuf::from(path), replacement.to_string()))
            .collect();
        redact_paths(message, &paths)
    }

    #[test]
    fn allowed_directory_paths_become_relative() {
        assert_eq!(
            redact(
                r#"IO Error: No files found that match the pattern "/srv/data/sales/2026.csv""#,
                &[("/srv/data", "")]
            ),
            r#"IO Error: No files found that match the pattern "sales/2026.csv""#
        );
        assert_eq!(redact("[/srv/data/]", &[("/srv/data/", "")]), "[.]");
        assert_eq!(redact("'/srv/data'", &[("/srv/data", "")]), "'.'");
    }

    #[test]
    fn named_replacement_keeps_the_rest_of_the_path() {
        assert_eq!(
            redact(
                r#"Cannot access directory "/home/flo/.duckdb/stored_secrets" - disabled"#,
                &[("/home/flo", "~")]
            ),
            r#"Cannot access directory "~/.duckdb/stored_secrets" - disabled"#
        );
        assert_eq!(
            redact(
                "/srv/db/app.duckdb is read-only",
                &[("/srv/db/app.duckdb", "app.duckdb")]
            ),
            "app.duckdb is read-only"
        );
    }

    #[test]
    fn longest_path_wins() {
        assert_eq!(
            redact(
                "'/home/flo/data/a.csv' and '/home/flo/.duckdb'",
                &[("/home/flo", "~"), ("/home/flo/data", "")]
            ),
            "'a.csv' and '~/.duckdb'"
        );
    }

    #[test]
    fn other_paths_are_kept() {
        for message in [
            r#""/srv/database/x.csv""#,
            r#""/other/srv/data/x.csv""#,
            "s3://bucket/srv/data/x.csv",
            "/etc/passwd",
        ] {
            assert_eq!(redact(message, &[("/srv/data", "")]), message);
        }
    }

    #[test]
    fn windows_paths() {
        assert_eq!(
            redact(r#""C:\data\sales.csv""#, &[(r"C:\data", "")]),
            r#""sales.csv""#
        );
    }
}
