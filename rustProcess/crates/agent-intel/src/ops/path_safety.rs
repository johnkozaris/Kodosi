use std::path::{Path, PathBuf};

const MAX_COMPONENT_BYTES: usize = 4096;

const FORBIDDEN_CHARS: &[char] = &[
    '/', '\\', ':', '\0', '\u{2044}', '\u{2215}', '\u{29F5}', '\u{29F9}',
];

#[derive(Debug, thiserror::Error)]
pub enum PathSafetyError {
    #[error("invalid input: {field} ({reason})")]
    InvalidInput { field: &'static str, reason: String },

    #[error("not found")]
    NotFound,

    #[error("permission denied")]
    PermissionDenied,

    #[error("io error")]
    Io,
}

impl PathSafetyError {
    pub fn invalid(field: &'static str, reason: impl Into<String>) -> Self {
        Self::InvalidInput {
            field,
            reason: reason.into(),
        }
    }
}

pub fn validate_component(field: &'static str, untrusted: &str) -> Result<(), PathSafetyError> {
    if untrusted.is_empty() {
        return Err(PathSafetyError::invalid(field, "empty"));
    }
    if untrusted.len() > MAX_COMPONENT_BYTES {
        return Err(PathSafetyError::invalid(field, "too long"));
    }
    if untrusted.contains("..") {
        return Err(PathSafetyError::invalid(field, "contains `..`"));
    }
    if untrusted.chars().any(char::is_control) {
        return Err(PathSafetyError::invalid(field, "contains control char"));
    }
    if untrusted.chars().any(|c| FORBIDDEN_CHARS.contains(&c)) {
        return Err(PathSafetyError::invalid(
            field,
            "contains path-separator char",
        ));
    }
    Ok(())
}

fn map_canonicalize_error<'a>(
    path: &'a Path,
    op: &'static str,
) -> impl FnOnce(std::io::Error) -> PathSafetyError + 'a {
    move |error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            PathSafetyError::NotFound
        } else {
            tracing::error!(%error, path = %path.display(), op, "canonicalize failed");
            PathSafetyError::Io
        }
    }
}

pub fn canonicalize_user_dir(
    field: &'static str,
    untrusted: &str,
) -> Result<PathBuf, PathSafetyError> {
    let path = validate_user_dir_input(field, untrusted)?;
    let canonical = path
        .canonicalize()
        .map_err(map_canonicalize_error(&path, "user dir"))?;
    if !canonical.is_dir() {
        return Err(PathSafetyError::invalid(field, "must be a directory"));
    }
    Ok(canonical)
}

fn validate_user_dir_input(
    field: &'static str,
    untrusted: &str,
) -> Result<PathBuf, PathSafetyError> {
    if untrusted.is_empty() {
        return Err(PathSafetyError::invalid(field, "empty"));
    }
    if untrusted.len() > MAX_COMPONENT_BYTES {
        return Err(PathSafetyError::invalid(field, "too long"));
    }
    if untrusted.chars().any(char::is_control) {
        return Err(PathSafetyError::invalid(field, "contains control char"));
    }
    let path = PathBuf::from(untrusted);
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(PathSafetyError::invalid(field, "contains `..` component"));
    }
    if !path.is_absolute() {
        return Err(PathSafetyError::invalid(field, "must be absolute"));
    }
    Ok(path)
}

pub fn validate_uuid(field: &'static str, value: &str) -> Result<(), PathSafetyError> {
    if value.len() != 36 {
        return Err(PathSafetyError::invalid(field, "must be 36-char UUID"));
    }
    for (idx, byte) in value.bytes().enumerate() {
        match idx {
            8 | 13 | 18 | 23 => {
                if byte != b'-' {
                    return Err(PathSafetyError::invalid(
                        field,
                        "malformed UUID punctuation",
                    ));
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() {
                    return Err(PathSafetyError::invalid(field, "not hex"));
                }
            }
        }
    }
    Ok(())
}

pub fn join_under_root(
    root: &Path,
    component: &str,
    allowed_ext: Option<&str>,
) -> Result<PathBuf, PathSafetyError> {
    if component.is_empty() {
        return Err(PathSafetyError::invalid("component", "empty"));
    }
    let canonical_root = root
        .canonicalize()
        .map_err(map_canonicalize_error(root, "root"))?;
    let joined = canonical_root.join(component);
    let canonical = joined
        .canonicalize()
        .map_err(map_canonicalize_error(&joined, "joined"))?;
    if !canonical.starts_with(&canonical_root) {
        tracing::warn!(
            canonical = %canonical.display(),
            root = %canonical_root.display(),
            "join_under_root rejected path outside root"
        );
        return Err(PathSafetyError::PermissionDenied);
    }
    if let Some(expected) = allowed_ext {
        let actual = canonical.extension().and_then(|e| e.to_str());
        if actual != Some(expected) {
            return Err(PathSafetyError::invalid(
                "extension",
                format!("expected .{expected}"),
            ));
        }
    }
    Ok(canonical)
}

pub fn enforce_under_root(root: &Path, target: &Path) -> Result<PathBuf, PathSafetyError> {
    let canonical_root = root
        .canonicalize()
        .map_err(map_canonicalize_error(root, "root"))?;
    let canonical = target
        .canonicalize()
        .map_err(map_canonicalize_error(target, "target"))?;
    if !canonical.starts_with(&canonical_root) {
        tracing::warn!(
            canonical = %canonical.display(),
            root = %canonical_root.display(),
            "enforce_under_root rejected path outside root"
        );
        return Err(PathSafetyError::PermissionDenied);
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_component_accepts_benign() {
        assert!(validate_component("slug", "my-project").is_ok());
        assert!(validate_component("slug", "unicode-ñ-é-あ").is_ok());
    }

    #[test]
    fn validate_component_rejects_dot_dot() {
        for case in ["..", "../", "../foo", "foo/..", "f..o"] {
            assert!(
                validate_component("slug", case).is_err(),
                "must reject {case:?}"
            );
        }
    }

    #[test]
    fn validate_uuid_accepts_canonical() {
        assert!(validate_uuid("id", "550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn validate_uuid_rejects_malformed() {
        assert!(validate_uuid("id", "550e8400e29b41d4a716446655440000").is_err());
    }
}
