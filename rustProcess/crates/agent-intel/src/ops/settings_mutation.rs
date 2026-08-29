use std::fs;
use std::io::Write as _;
use std::path::Path;

use jsonc_parser::ParseOptions;
use jsonc_parser::cst::CstRootNode;

const MAX_SETTINGS_MUTATION_ATTEMPTS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
enum SettingsSnapshot {
    Missing,
    Present(Vec<u8>),
}

enum CommitOutcome {
    Written,
    Conflict,
}

pub(crate) fn parse_options() -> ParseOptions {
    ParseOptions {
        allow_comments: true,
        allow_loose_object_property_names: false,
        allow_trailing_commas: true,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    }
}

pub(crate) fn mutate_settings_file(
    path: &Path,
    mutation: impl Fn(&CstRootNode) -> Result<(), String>,
) -> Result<(), String> {
    crate::ops::settings_lock::with_lock(path, || {
        mutate_settings_file_with_committer(path, mutation, commit_after_revalidation)
    })
}

fn mutate_settings_file_with_committer(
    path: &Path,
    mutation: impl Fn(&CstRootNode) -> Result<(), String>,
    mut commit: impl FnMut(&Path, &SettingsSnapshot, &[u8]) -> Result<CommitOutcome, String>,
) -> Result<(), String> {
    for _ in 0..MAX_SETTINGS_MUTATION_ATTEMPTS {
        let snapshot = read_snapshot(path)?;
        let text = match &snapshot {
            SettingsSnapshot::Missing => "{}",
            SettingsSnapshot::Present(bytes) => std::str::from_utf8(bytes)
                .map_err(|error| format!("parse {} as UTF-8: {error}", path.display()))?,
        };
        let root = CstRootNode::parse(text, &parse_options())
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        if root.object_value().is_none() {
            return Err(format!("{} root must be an object", path.display()));
        }
        mutation(&root)?;
        let candidate = root.to_string().into_bytes();
        match commit(path, &snapshot, &candidate)? {
            CommitOutcome::Written => return Ok(()),
            CommitOutcome::Conflict => {}
        }
    }

    Err(format!(
        "{} changed during all {MAX_SETTINGS_MUTATION_ATTEMPTS} mutation attempts",
        path.display()
    ))
}

fn read_snapshot(path: &Path) -> Result<SettingsSnapshot, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(SettingsSnapshot::Present(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SettingsSnapshot::Missing),
        Err(error) => Err(format!("read {}: {error}", path.display())),
    }
}

fn commit_after_revalidation(
    path: &Path,
    expected: &SettingsSnapshot,
    candidate: &[u8],
) -> Result<CommitOutcome, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "settings path has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| format!("mkdir {}: {error}", parent.display()))?;

    let existing_permissions = match fs::metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("stat {}: {error}", path.display())),
    };
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("create settings tempfile: {error}"))?;
    temporary
        .write_all(candidate)
        .map_err(|error| format!("write settings tempfile: {error}"))?;
    if let Some(permissions) = existing_permissions {
        temporary
            .as_file()
            .set_permissions(permissions)
            .map_err(|error| format!("set settings tempfile permissions: {error}"))?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("sync settings tempfile: {error}"))?;

    if &read_snapshot(path)? != expected {
        return Ok(CommitOutcome::Conflict);
    }

    temporary
        .persist(path)
        .map_err(|error| format!("replace {}: {}", path.display(), error.error))?;
    sync_parent(parent)?;
    Ok(CommitOutcome::Written)
}

fn sync_parent(parent: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("sync directory {}: {error}", parent.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_rebases_patch_onto_external_edit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, "{\n  // retained\n  \"unknown\": true\n}\n").unwrap();
        let mut attempts = 0;

        mutate_settings_file_with_committer(
            &path,
            |root| {
                let object = root.object_value().expect("object");
                match object.get("owned") {
                    Some(property) => property.set_value("updated".into()),
                    None => {
                        object.append("owned", "updated".into());
                    }
                }
                Ok(())
            },
            |path, expected, candidate| {
                attempts += 1;
                if attempts == 1 {
                    fs::write(
                        path,
                        "{\n  // retained\n  \"unknown\": true,\n  \"external\": 7\n}\n",
                    )
                    .unwrap();
                    return Ok(CommitOutcome::Conflict);
                }
                commit_after_revalidation(path, expected, candidate)
            },
        )
        .expect("mutation succeeds after rebase");

        let written = fs::read_to_string(path).unwrap();
        assert_eq!(attempts, 2);
        assert!(written.contains("// retained"));
        assert!(written.contains("\"unknown\": true"));
        assert!(written.contains("\"external\": 7"));
        assert!(written.contains("\"owned\": \"updated\""));
    }

    #[test]
    fn missing_file_creation_conflict_rebases() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut attempts = 0;

        mutate_settings_file_with_committer(
            &path,
            |root| {
                root.object_value()
                    .expect("object")
                    .append("owned", true.into());
                Ok(())
            },
            |path, expected, candidate| {
                attempts += 1;
                if attempts == 1 {
                    fs::write(path, "{ \"external\": true }\n").unwrap();
                    return Ok(CommitOutcome::Conflict);
                }
                commit_after_revalidation(path, expected, candidate)
            },
        )
        .expect("mutation succeeds after creation conflict");

        let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(value["external"], true);
        assert_eq!(value["owned"], true);
    }

    #[test]
    fn conflict_retries_are_bounded_and_non_destructive() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, "{}\n").unwrap();
        let mut attempts = 0;

        let error = mutate_settings_file_with_committer(
            &path,
            |root| {
                root.object_value()
                    .expect("object")
                    .append("owned", true.into());
                Ok(())
            },
            |path, _, _| {
                attempts += 1;
                fs::write(path, format!("{{\"external\":{attempts}}}\n")).unwrap();
                Ok(CommitOutcome::Conflict)
            },
        )
        .expect_err("continuous churn must fail");

        assert_eq!(attempts, MAX_SETTINGS_MUTATION_ATTEMPTS);
        assert!(error.contains("4 mutation attempts"));
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            format!("{{\"external\":{MAX_SETTINGS_MUTATION_ATTEMPTS}}}\n")
        );
    }

    #[test]
    fn scalar_root_is_rejected_without_rewrite() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, "42\n").unwrap();

        let error = mutate_settings_file(&path, |_| Ok(())).expect_err("scalar must fail");

        assert!(error.contains("root must be an object"));
        assert_eq!(fs::read_to_string(path).unwrap(), "42\n");
    }

    #[cfg(unix)]
    #[test]
    fn production_commit_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, "{}\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        mutate_settings_file(&path, |root| {
            root.object_value()
                .expect("object")
                .append("owned", true.into());
            Ok(())
        })
        .unwrap();

        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}
