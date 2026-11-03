use std::path::{Component, Path, PathBuf};

use crate::error::{ArchonError, ArchonResult};

/// Validate a name that will be created on disk.
///
/// Archon treats archives as untrusted input. This rejects absolute paths,
/// parent traversal, Windows drive/UNC paths, alternate data streams, reserved
/// device names and components Windows would silently rewrite.
pub fn sanitize_entry_path(raw: &str) -> ArchonResult<PathBuf> {
    if raw.is_empty() {
        return Err(ArchonError::PathTraversal {
            entry: raw.to_string(),
        });
    }
    if raw.contains('\0') {
        return Err(ArchonError::PathTraversal {
            entry: raw.to_string(),
        });
    }

    // ZIP uses `/`, but archives produced on Windows frequently contain `\`.
    // Treat both as separators so a backslash cannot smuggle `..` past us.
    let normalized = raw.replace('\\', "/");

    if normalized.starts_with('/') {
        return Err(ArchonError::PathTraversal {
            entry: raw.to_string(),
        });
    }
    let bytes = normalized.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return Err(ArchonError::PathTraversal {
            entry: raw.to_string(),
        });
    }

    let mut safe = PathBuf::new();
    for component in normalized.split('/') {
        match component {
            "" | "." => continue,
            ".." => {
                return Err(ArchonError::PathTraversal {
                    entry: raw.to_string(),
                });
            }
            value => {
                validate_component(value, raw)?;
                safe.push(value);
            }
        }
    }

    if safe.as_os_str().is_empty() {
        return Err(ArchonError::PathTraversal {
            entry: raw.to_string(),
        });
    }

    Ok(safe)
}

fn validate_component(component: &str, original: &str) -> ArchonResult<()> {
    if component.chars().any(|ch| ch.is_control()) {
        return Err(ArchonError::PathTraversal {
            entry: original.to_string(),
        });
    }

    if cfg!(windows) {
        const INVALID: &[char] = &['<', '>', ':', '"', '|', '?', '*'];
        if component.chars().any(|ch| INVALID.contains(&ch)) {
            return Err(ArchonError::PathTraversal {
                entry: original.to_string(),
            });
        }
        if component.ends_with(' ') || component.ends_with('.') {
            return Err(ArchonError::PathTraversal {
                entry: original.to_string(),
            });
        }

        let stem = component
            .split('.')
            .next()
            .unwrap_or(component)
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        let reserved = matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        );
        if reserved {
            return Err(ArchonError::PathTraversal {
                entry: original.to_string(),
            });
        }
    }

    Ok(())
}

/// Join a validated archive path to a destination and prove the result stays
/// inside that destination. Components are already validated; this is a second
/// line of defence and also protects callers that bypass [`sanitize_entry_path`].
pub fn safe_join(destination: &Path, relative: &Path) -> ArchonResult<PathBuf> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(ArchonError::PathTraversal {
            entry: relative.display().to_string(),
        });
    }

    let root = destination
        .canonicalize()
        .map_err(|error| ArchonError::Io(format!("Cannot resolve destination: {error}")))?;
    let joined = root.join(relative);

    // The file may not exist yet, so canonicalize the nearest existing
    // ancestor and append the remaining components.
    let mut existing = joined.as_path();
    let mut tail = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            return Err(ArchonError::PathTraversal {
                entry: relative.display().to_string(),
            });
        };
        tail.push(name.to_owned());
        let Some(parent) = existing.parent() else {
            return Err(ArchonError::PathTraversal {
                entry: relative.display().to_string(),
            });
        };
        existing = parent;
    }

    let mut resolved = existing
        .canonicalize()
        .map_err(|error| ArchonError::Io(format!("Cannot resolve destination: {error}")))?;
    for part in tail.into_iter().rev() {
        resolved.push(part);
    }

    if !resolved.starts_with(&root) {
        return Err(ArchonError::PathTraversal {
            entry: relative.display().to_string(),
        });
    }

    Ok(joined)
}

/// Windows path comparison is case-insensitive; Linux is not.
pub fn path_key(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

pub fn normalize_archive_path(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_regular_nested_paths() {
        assert_eq!(
            sanitize_entry_path("folder/sub/file.txt").unwrap(),
            PathBuf::from("folder").join("sub").join("file.txt")
        );
        assert_eq!(
            sanitize_entry_path("folder\\file.txt").unwrap(),
            PathBuf::from("folder").join("file.txt")
        );
    }

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        for candidate in [
            "../evil.txt",
            "a/../../evil.txt",
            "/etc/passwd",
            "\\\\server\\share\\evil.txt",
            "//server/share/evil.txt",
            "C:\\Windows\\evil.txt",
            "C:evil.txt",
            "..\\evil.txt",
            "a/b/../../../evil.txt",
        ] {
            assert!(
                sanitize_entry_path(candidate).is_err(),
                "expected rejection: {candidate}"
            );
        }
    }

    #[test]
    fn rejects_windows_specific_hazards() {
        for candidate in [
            "file.txt:stream",
            "CON",
            "nul.txt",
            "folder/trailing.",
            "folder/trailing ",
            "bad?name.txt",
            "bad*name.txt",
        ] {
            assert!(
                sanitize_entry_path(candidate).is_err(),
                "expected rejection: {candidate}"
            );
        }
    }

    #[test]
    fn rejects_empty_and_dot_only() {
        for candidate in ["", "/", "./", "a/.."] {
            assert!(sanitize_entry_path(candidate).is_err());
        }
    }
}
