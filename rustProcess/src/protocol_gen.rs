use std::{
    fs,
    path::{Path, PathBuf},
};

const USAGE: &str = "usage: protocol-gen <write|check|check-compat|freeze-current>";

fn main() -> kodosi_runtime::Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    if args.next().is_some() {
        return Err(kodosi_runtime::AppError::Unsupported {
            reason: USAGE.to_owned(),
        });
    }
    match command.as_deref() {
        Some("write") => write_desktop_runtime_authority_artifacts(),
        Some("check") => check_desktop_runtime_authority_artifacts(),
        Some("check-compat") => check_compat_against_frozen_snapshots(),
        Some("freeze-current") => freeze_current_snapshot(),
        _ => Err(kodosi_runtime::AppError::Unsupported {
            reason: USAGE.to_owned(),
        }),
    }
}

const AUTHORITY_JSON_REL: &str = "protocol/desktop-runtime-authority.json";
const VERSION_SNAPSHOTS_REL: &str = "protocol/versions";

fn repo_root() -> kodosi_runtime::Result<&'static Path> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .ok_or_else(|| kodosi_runtime::AppError::Unsupported {
            reason: "rustProcess manifest has no repository parent directory".to_owned(),
        })
}

fn freeze_current_snapshot() -> kodosi_runtime::Result<()> {
    let path = repo_root()?.join(format!(
        "{VERSION_SNAPSHOTS_REL}/v{}.json",
        kodosi_runtime::protocol_authority::PROTOCOL_VERSION
    ));
    let generated = kodosi_runtime::protocol_authority::render_desktop_runtime_authority_json()?;
    freeze_snapshot_at(&path, &generated)
}

fn freeze_snapshot_at(path: &Path, generated: &str) -> kodosi_runtime::Result<()> {
    match fs::read_to_string(path) {
        Ok(existing) if existing == generated => Ok(()),
        Ok(_) => Err(kodosi_runtime::AppError::Unsupported {
            reason: format!(
                "{} already exists and is immutable; bump PROTOCOL_VERSION before freezing a new lineage",
                path.display()
            ),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::write(path, generated)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn write_desktop_runtime_authority_artifacts() -> kodosi_runtime::Result<()> {
    let path = repo_root()?.join(AUTHORITY_JSON_REL);
    fs::write(
        &path,
        kodosi_runtime::protocol_authority::render_desktop_runtime_authority_json()?,
    )?;
    Ok(())
}

fn check_desktop_runtime_authority_artifacts() -> kodosi_runtime::Result<()> {
    let path = repo_root()?.join(AUTHORITY_JSON_REL);
    let on_disk = fs::read_to_string(&path)?;
    let generated = kodosi_runtime::protocol_authority::render_desktop_runtime_authority_json()?;
    if on_disk == generated {
        return Ok(());
    }
    Err(kodosi_runtime::AppError::Unsupported {
        reason: format!(
            "{AUTHORITY_JSON_REL} is out of date. Run `cargo run -q --bin protocol-gen -- write` from rustProcess and commit the result."
        ),
    })
}

fn check_compat_against_frozen_snapshots() -> kodosi_runtime::Result<()> {
    let generated = kodosi_runtime::protocol_authority::render_desktop_runtime_authority_json()?;
    let current: serde_json::Value = serde_json::from_str(&generated)?;
    let current_version = protocol_version(&current, "generated protocol authority")?;
    let current_path =
        repo_root()?.join(format!("{VERSION_SNAPSHOTS_REL}/v{current_version}.json"));
    let frozen_current = fs::read_to_string(&current_path).map_err(|error| {
        kodosi_runtime::AppError::Unsupported {
            reason: format!(
                "current protocol v{current_version} has no readable exact snapshot at {}: {error}",
                current_path.display()
            ),
        }
    })?;
    check_current_snapshot_exact(current_version, &generated, &frozen_current)?;
    let snapshots = frozen_snapshot_paths()?
        .into_iter()
        .map(|path| {
            let snapshot_name = path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or("unknown snapshot")
                .to_owned();
            let frozen: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path)?)
                .map_err(|error| kodosi_runtime::AppError::Unsupported {
                    reason: format!("{snapshot_name} malformed: {error}"),
                })?;
            FrozenSnapshot::new(snapshot_name, frozen)
        })
        .collect::<kodosi_runtime::Result<Vec<_>>>()?;
    check_compatibility(&current, &snapshots)
}

fn check_current_snapshot_exact(
    version: u64,
    generated: &str,
    frozen: &str,
) -> kodosi_runtime::Result<()> {
    if generated == frozen {
        return Ok(());
    }
    Err(kodosi_runtime::AppError::Unsupported {
        reason: format!(
            "generated current protocol authority differs from immutable v{version}.json; restore the frozen v{version} shape or bump PROTOCOL_VERSION and freeze a new lineage"
        ),
    })
}

struct FrozenSnapshot {
    name: String,
    version: u64,
    value: serde_json::Value,
}

impl FrozenSnapshot {
    fn new(name: String, value: serde_json::Value) -> kodosi_runtime::Result<Self> {
        let version = protocol_version(&value, &name)?;
        let expected_name = format!("v{version}.json");
        if name != expected_name {
            return Err(kodosi_runtime::AppError::Unsupported {
                reason: format!(
                    "protocol snapshot {name} declares version {version}; expected file name {expected_name}"
                ),
            });
        }
        Ok(Self {
            name,
            version,
            value,
        })
    }
}

fn protocol_version(value: &serde_json::Value, label: &str) -> kodosi_runtime::Result<u64> {
    value
        .get("protocolVersion")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| kodosi_runtime::AppError::Unsupported {
            reason: format!("{label} has no unsigned protocolVersion"),
        })
}

fn check_compatibility(
    current: &serde_json::Value,
    snapshots: &[FrozenSnapshot],
) -> kodosi_runtime::Result<()> {
    let current_version = protocol_version(current, "generated protocol authority")?;
    let latest_frozen = snapshots
        .iter()
        .map(|snapshot| snapshot.version)
        .max()
        .unwrap_or(0);
    if current_version < latest_frozen {
        return Err(kodosi_runtime::AppError::Unsupported {
            reason: format!(
                "generated protocol version {current_version} is older than frozen history v{latest_frozen}"
            ),
        });
    }

    if let Some(frozen) = snapshots
        .iter()
        .find(|snapshot| snapshot.version == current_version)
    {
        let report =
            kodosi_runtime::protocol_authority::compatibility_report(&frozen.value, current);
        if report.is_compatible() {
            return Ok(());
        }
        return Err(kodosi_runtime::AppError::Unsupported {
            reason: format!(
                "protocol regression within v{current_version} vs {}: removed_fields={:?}, removed_entries={:?}, added_required_fields={:?}. \
                 Restore compatibility or bump PROTOCOL_VERSION to start a new lineage.",
                frozen.name,
                report.removed_fields,
                report.removed_entries,
                report.added_required_fields
            ),
        });
    }

    Err(kodosi_runtime::AppError::Unsupported {
        reason: format!(
            "generated protocol version {current_version} has no immutable snapshot, while frozen history reaches v{latest_frozen}"
        ),
    })
}

fn frozen_snapshot_paths() -> kodosi_runtime::Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(repo_root()?.join(VERSION_SNAPSHOTS_REL))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|path| path.extension().is_some_and(|ext| ext == "json"));
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(version: u64, commands: &[&str]) -> FrozenSnapshot {
        FrozenSnapshot::new(
            format!("v{version}.json"),
            json!({"protocolVersion": version, "commands": commands}),
        )
        .expect("valid snapshot")
    }

    #[test]
    fn current_snapshot_requires_byte_exact_parity() {
        check_current_snapshot_exact(3, "{\"protocolVersion\":3}\n", "{\"protocolVersion\":3}\n")
            .expect("identical current snapshot");
        let error = check_current_snapshot_exact(
            3,
            "{\"protocolVersion\":3,\"optional\":true}\n",
            "{\"protocolVersion\":3}\n",
        )
        .expect_err("same-version additive drift must require a version bump");
        std::assert_matches!(
            error,
            kodosi_runtime::AppError::Unsupported { reason }
                if reason.contains("differs from immutable v3.json")
        );
    }

    #[test]
    fn same_version_breaking_change_fails() {
        let current = json!({"protocolVersion": 3, "commands": ["kept"]});
        let error = check_compatibility(
            &current,
            &[
                snapshot(2, &["historical"]),
                snapshot(3, &["kept", "removed"]),
            ],
        )
        .expect_err("same-version removal must fail");

        std::assert_matches!(
            error,
            kodosi_runtime::AppError::Unsupported { reason }
                if reason.contains("within v3") && reason.contains("removed")
        );
    }

    #[test]
    fn deliberate_version_bump_succeeds_only_with_matching_new_snapshot() {
        let current = json!({"protocolVersion": 4, "commands": ["replacement"]});
        check_compatibility(
            &current,
            &[
                snapshot(2, &["legacy-a"]),
                snapshot(3, &["legacy-b"]),
                snapshot(4, &["replacement"]),
            ],
        )
        .expect("a frozen matching v4 snapshot enables the new lineage");
    }

    #[test]
    fn version_bump_without_current_snapshot_fails_closed() {
        let current = json!({"protocolVersion": 4, "commands": ["replacement"]});
        let error = check_compatibility(
            &current,
            &[snapshot(2, &["legacy-a"]), snapshot(3, &["legacy-b"])],
        )
        .expect_err("a version bump without v4.json must not disable compatibility");
        std::assert_matches!(
            error,
            kodosi_runtime::AppError::Unsupported { reason }
                if reason.contains("version 4 has no immutable snapshot")
                    && reason.contains("v3")
        );
    }

    #[test]
    fn version_behind_frozen_history_fails_without_matching_snapshot() {
        let current = json!({"protocolVersion": 2, "commands": ["a"]});
        let error = check_compatibility(&current, &[snapshot(3, &["b"])])
            .expect_err("rollback cannot bypass its missing lineage snapshot");
        std::assert_matches!(
            error,
            kodosi_runtime::AppError::Unsupported { reason }
                if reason.contains("older than frozen history v3")
        );
    }

    #[test]
    fn matching_old_snapshot_cannot_roll_back_newer_history() {
        let current = json!({"protocolVersion": 3, "commands": ["old"]});
        let error = check_compatibility(&current, &[snapshot(3, &["old"]), snapshot(4, &["new"])])
            .expect_err("matching an old snapshot must not authorize rollback");
        std::assert_matches!(
            error,
            kodosi_runtime::AppError::Unsupported { reason }
                if reason.contains("older than frozen history v4")
        );
    }

    #[test]
    fn freezing_never_rewrites_historical_snapshot() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("v3.json");
        let historical = "{\"protocolVersion\":3,\"commands\":[\"old\"]}";
        fs::write(&path, historical).expect("seed snapshot");

        let error = freeze_snapshot_at(
            &path,
            "{\"protocolVersion\":3,\"commands\":[\"replacement\"]}",
        )
        .expect_err("existing snapshot is immutable");
        std::assert_matches!(
            error,
            kodosi_runtime::AppError::Unsupported { reason }
                if reason.contains("immutable")
        );
        assert_eq!(fs::read_to_string(path).expect("read snapshot"), historical);
    }
}
