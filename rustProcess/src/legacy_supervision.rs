use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{AppError, Result};

const PREVIOUSLY_SHIPPED_LEGACY_ROOM_SKILL: &[u8] = br"---
name: kodosi-room
description: Coordinate with reachable Kodosi room members through the kodosi CLI.
allowed-tools: [Bash]
---

# Kodosi rooms

The `kodosi` CLI is the contract. Discover agents, messages, room chat, and
room tasks through `kodosi --help` and nested `--help`. Replies and assigned
task updates arrive as session context; do not poll.
";
const AUTOMATIC_CLEANUP_MARKER: &[u8] = b"legacy-supervision-cleanup-v1\n";
const AUTOMATIC_CLEANUP_MARKER_NAME: &str = "legacy-supervision-cleanup-v1.complete";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacySupervisionCleanupReport {
    pub(crate) dry_run: bool,
    pub(crate) removed_entries: usize,
    pub(crate) changed_paths: Vec<String>,
    pub(crate) helper_removed: bool,
    pub(crate) skills_removed: usize,
    pub(crate) backup_directory: Option<String>,
}

struct PlannedSettingsCleanup {
    path: PathBuf,
    removal: agent_intel::ops::legacy_hooks::LegacyHookRemoval,
    delete_when_clean: bool,
}

#[cfg(feature = "cli")]
pub(crate) fn cleanup_default_home(
    dry_run: bool,
) -> Result<Option<LegacySupervisionCleanupReport>> {
    let Some(base_dirs) = directories::BaseDirs::new() else {
        return Ok(None);
    };
    cleanup(base_dirs.home_dir(), dry_run).map(Some)
}

pub(crate) fn cleanup_default_home_once() -> Result<Option<LegacySupervisionCleanupReport>> {
    let Some(base_dirs) = directories::BaseDirs::new() else {
        return Ok(None);
    };
    cleanup_once(base_dirs.home_dir())
}

fn cleanup_once(home: &Path) -> Result<Option<LegacySupervisionCleanupReport>> {
    let marker = home
        .join(".kodosi")
        .join("install")
        .join(AUTOMATIC_CLEANUP_MARKER_NAME);
    match fs::symlink_metadata(&marker) {
        Ok(metadata)
            if metadata.is_file()
                && metadata.len()
                    <= u64::try_from(AUTOMATIC_CLEANUP_MARKER.len()).unwrap_or(u64::MAX)
                && fs::read(&marker).is_ok_and(|bytes| bytes == AUTOMATIC_CLEANUP_MARKER) =>
        {
            return Ok(None);
        }
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(AppError::Unsupported {
                reason: format!(
                    "refusing automatic legacy cleanup marker at non-regular path {}",
                    marker.display()
                ),
            });
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(AppError::Io(error)),
    }

    let report = cleanup(home, false)?;
    let marker_dir = marker.parent().ok_or_else(|| AppError::Unsupported {
        reason: "automatic legacy cleanup marker has no parent".to_owned(),
    })?;
    crate::support::platform::fs::ensure_dir(marker_dir)?;
    crate::support::storage::atomic_file::atomic_write(
        &marker,
        AUTOMATIC_CLEANUP_MARKER,
        crate::support::storage::atomic_file::FileMode::UserPrivate,
    )?;
    Ok(Some(report))
}

#[expect(
    clippy::too_many_lines,
    reason = "cleanup plans every mutation before applying any of them"
)]
pub(crate) fn cleanup(home: &Path, dry_run: bool) -> Result<LegacySupervisionCleanupReport> {
    let hook_binary = home.join(".kodosi").join("bin").join("kodosi-hook");
    let known_legacy_skills: [&[u8]; 2] = [
        include_bytes!("../skills/kodosi-room/SKILL.md"),
        PREVIOUSLY_SHIPPED_LEGACY_ROOM_SKILL,
    ];
    let skill_paths = [
        home.join(".claude/skills/kodosi-room/SKILL.md"),
        home.join(".copilot/skills/kodosi-room/SKILL.md"),
    ];
    let targets = [
        PlannedSettingsCleanup {
            path: home.join(".claude").join("settings.json"),
            removal: inspect_regular_settings(
                &home.join(".claude").join("settings.json"),
                &hook_binary,
            )?,
            delete_when_clean: false,
        },
        PlannedSettingsCleanup {
            path: home.join(".copilot").join("settings.json"),
            removal: inspect_regular_settings(
                &home.join(".copilot").join("settings.json"),
                &hook_binary,
            )?,
            delete_when_clean: false,
        },
        PlannedSettingsCleanup {
            path: home.join(".copilot").join("hooks").join("kodosi.json"),
            removal: inspect_regular_settings(
                &home.join(".copilot").join("hooks").join("kodosi.json"),
                &hook_binary,
            )?,
            delete_when_clean: true,
        },
    ];
    inspect_regular_file(&hook_binary)?;
    for path in &skill_paths {
        inspect_regular_file(path)?;
    }
    let owned_skills = skill_paths
        .iter()
        .filter(|path| {
            fs::read(path).is_ok_and(|bytes| {
                known_legacy_skills
                    .iter()
                    .any(|legacy_skill| bytes == *legacy_skill)
            })
        })
        .cloned()
        .collect::<Vec<_>>();

    let removed_entries = targets
        .iter()
        .map(|target| target.removal.removed_entries)
        .sum();
    let helper_exists = hook_binary.is_file();
    if targets
        .iter()
        .any(|target| target.removal.remaining_reference)
    {
        return Err(AppError::Unsupported {
            reason: "legacy Kodosi hook cleanup found an ambiguous helper reference; no files were changed"
                .to_owned(),
        });
    }

    let mut changed_paths = targets
        .iter()
        .filter(|target| target.removal.removed_entries > 0)
        .map(|target| target.path.display().to_string())
        .collect::<Vec<_>>();
    if helper_exists {
        changed_paths.push(hook_binary.display().to_string());
    }
    changed_paths.extend(owned_skills.iter().map(|path| path.display().to_string()));
    if dry_run || changed_paths.is_empty() {
        return Ok(LegacySupervisionCleanupReport {
            dry_run,
            removed_entries,
            changed_paths,
            helper_removed: false,
            skills_removed: 0,
            backup_directory: None,
        });
    }

    let backup_directory = home.join(".kodosi").join("install").join(format!(
        "legacy-hook-cleanup-{}",
        time::OffsetDateTime::now_utc().unix_timestamp_nanos()
    ));
    crate::support::platform::fs::ensure_dir(&backup_directory)?;
    for target in &targets {
        if target.removal.removed_entries > 0 {
            backup_regular_file(&target.path, &backup_directory)?;
        }
    }
    if helper_exists {
        backup_regular_file(&hook_binary, &backup_directory)?;
    }
    for (index, skill) in owned_skills.iter().enumerate() {
        fs::copy(
            skill,
            backup_directory.join(format!("room-skill-{index}-SKILL.md")),
        )
        .map_err(AppError::Io)?;
    }

    for target in &targets {
        if target.removal.removed_entries == 0 {
            continue;
        }
        let cleaned =
            agent_intel::ops::legacy_hooks::remove_from_settings(&target.path, &hook_binary)
                .map_err(|reason| AppError::Unsupported { reason })?;
        if cleaned.remaining_reference {
            return Err(AppError::Unsupported {
                reason: format!(
                    "legacy hook reference remained in {} after cleanup",
                    target.path.display()
                ),
            });
        }
        if target.delete_when_clean
            && cleaned.hooks_empty
            && is_owned_copilot_hook_file(&target.path)
        {
            fs::remove_file(&target.path).map_err(AppError::Io)?;
        }
    }

    let remaining_reference = targets.iter().try_fold(false, |found, target| {
        let content = match fs::read_to_string(&target.path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(AppError::Io(error)),
        };
        Ok::<_, AppError>(found || content.contains(hook_binary.to_string_lossy().as_ref()))
    })?;
    if remaining_reference {
        return Err(AppError::Unsupported {
            reason: "legacy hook references remain; preserved the helper binary".to_owned(),
        });
    }

    let helper_removed = if helper_exists {
        fs::remove_file(&hook_binary).map_err(AppError::Io)?;
        if let Some(parent) = hook_binary.parent() {
            drop(fs::remove_dir(parent));
        }
        true
    } else {
        false
    };
    for skill in &owned_skills {
        fs::remove_file(skill).map_err(AppError::Io)?;
        if let Some(parent) = skill.parent() {
            drop(fs::remove_dir(parent));
        }
    }

    Ok(LegacySupervisionCleanupReport {
        dry_run,
        removed_entries,
        changed_paths: std::mem::take(&mut changed_paths),
        helper_removed,
        skills_removed: owned_skills.len(),
        backup_directory: Some(backup_directory.display().to_string()),
    })
}

fn inspect_regular_settings(
    path: &Path,
    hook_binary: &Path,
) -> Result<agent_intel::ops::legacy_hooks::LegacyHookRemoval> {
    inspect_regular_file(path)?;
    agent_intel::ops::legacy_hooks::inspect_settings(path, hook_binary)
        .map_err(|reason| AppError::Unsupported { reason })
}

fn inspect_regular_file(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(AppError::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::Unsupported {
            reason: format!(
                "refusing legacy cleanup for non-regular file {}",
                path.display()
            ),
        });
    }
    Ok(())
}

fn backup_regular_file(path: &Path, backup_directory: &Path) -> Result<()> {
    let Some(file_name) = path.file_name() else {
        return Err(AppError::Unsupported {
            reason: format!("legacy cleanup path has no file name: {}", path.display()),
        });
    };
    let parent_name = path
        .parent()
        .and_then(Path::file_name)
        .unwrap_or_default()
        .to_string_lossy();
    let backup_name = format!("{parent_name}-{}", file_name.to_string_lossy());
    fs::copy(path, backup_directory.join(backup_name)).map_err(AppError::Io)?;
    Ok(())
}

fn is_owned_copilot_hook_file(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .keys()
        .all(|key| matches!(key.as_str(), "version" | "hooks"))
        && object.get("version").and_then(serde_json::Value::as_u64) == Some(1)
        && object
            .get("hooks")
            .is_none_or(|hooks| hooks.as_object().is_some_and(serde_json::Map::is_empty))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_legacy_install(home: &Path) {
        let hook = home.join(".kodosi/bin/kodosi-hook");
        fs::create_dir_all(hook.parent().unwrap()).unwrap();
        fs::write(&hook, b"legacy helper").unwrap();

        let claude = home.join(".claude/settings.json");
        fs::create_dir_all(claude.parent().unwrap()).unwrap();
        fs::write(
            claude,
            format!(
                r#"{{
  // preserve
  "theme": "dark",
  "hooks": {{
    "PreToolUse": [
      {{"matcher":"*","hooks":[{{"type":"command","command":"'{}'","timeout":600}}]}},
      {{"matcher":"Bash","hooks":[{{"type":"command","command":"foreign"}}]}}
    ]
  }}
}}"#,
                hook.display()
            ),
        )
        .unwrap();

        let copilot = home.join(".copilot/hooks/kodosi.json");
        fs::create_dir_all(copilot.parent().unwrap()).unwrap();
        fs::write(
            copilot,
            format!(
                r#"{{"version":1,"hooks":{{"preToolUse":[{{"type":"command","bash":"'{}' --dialect copilot --event preToolUse","powershell":"& '{}' --dialect copilot --event preToolUse","timeoutSec":600}}]}}}}"#,
                hook.display(),
                hook.display()
            ),
        )
        .unwrap();

        for skill in [
            home.join(".claude/skills/kodosi-room/SKILL.md"),
            home.join(".copilot/skills/kodosi-room/SKILL.md"),
        ] {
            fs::create_dir_all(skill.parent().unwrap()).unwrap();
            fs::write(skill, include_bytes!("../skills/kodosi-room/SKILL.md")).unwrap();
        }
    }

    #[test]
    fn cleanup_is_exact_backed_up_and_idempotent() {
        let home = tempfile::tempdir().unwrap();
        write_legacy_install(home.path());

        let first = cleanup(home.path(), false).unwrap();
        assert_eq!(first.removed_entries, 2);
        assert!(first.helper_removed);
        assert!(first.backup_directory.is_some());
        assert!(!home.path().join(".kodosi/bin/kodosi-hook").exists());
        assert!(!home.path().join(".copilot/hooks/kodosi.json").exists());
        let claude = fs::read_to_string(home.path().join(".claude/settings.json")).unwrap();
        assert!(claude.contains("// preserve"));
        assert!(claude.contains("\"command\":\"foreign\""));
        assert!(!claude.contains("kodosi-hook"));
        assert_eq!(first.skills_removed, 2);
        assert!(
            !home
                .path()
                .join(".claude/skills/kodosi-room/SKILL.md")
                .exists()
        );
        assert!(
            !home
                .path()
                .join(".copilot/skills/kodosi-room/SKILL.md")
                .exists()
        );

        let second = cleanup(home.path(), false).unwrap();
        assert_eq!(second.removed_entries, 0);
        assert!(!second.helper_removed);
        assert_eq!(second.skills_removed, 0);
        assert!(second.changed_paths.is_empty());
    }

    #[test]
    fn automatic_cleanup_is_durably_one_shot_but_explicit_cleanup_can_rerun() {
        let home = tempfile::tempdir().unwrap();
        write_legacy_install(home.path());

        let first = cleanup_once(home.path())
            .unwrap()
            .expect("first automatic run");
        assert!(first.helper_removed);
        let marker = home
            .path()
            .join(".kodosi/install")
            .join(AUTOMATIC_CLEANUP_MARKER_NAME);
        assert_eq!(fs::read(&marker).unwrap(), AUTOMATIC_CLEANUP_MARKER);

        write_legacy_install(home.path());
        assert!(cleanup_once(home.path()).unwrap().is_none());
        assert!(home.path().join(".kodosi/bin/kodosi-hook").exists());

        let explicit = cleanup(home.path(), false).unwrap();
        assert!(explicit.helper_removed);
    }

    #[test]
    fn dry_run_changes_nothing() {
        let home = tempfile::tempdir().unwrap();
        write_legacy_install(home.path());
        let before = fs::read(home.path().join(".claude/settings.json")).unwrap();

        let report = cleanup(home.path(), true).unwrap();

        assert!(report.dry_run);
        assert_eq!(report.removed_entries, 2);
        assert_eq!(
            fs::read(home.path().join(".claude/settings.json")).unwrap(),
            before
        );
        assert!(home.path().join(".kodosi/bin/kodosi-hook").exists());
    }

    #[test]
    fn removes_previous_legacy_skill_copy() {
        let home = tempfile::tempdir().unwrap();
        let skill = home.path().join(".claude/skills/kodosi-room/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, PREVIOUSLY_SHIPPED_LEGACY_ROOM_SKILL).unwrap();

        let report = cleanup(home.path(), false).unwrap();

        assert_eq!(report.skills_removed, 1);
        assert!(!skill.exists());
    }

    #[test]
    fn preserves_edited_previous_legacy_skill_copy() {
        let home = tempfile::tempdir().unwrap();
        let skill = home.path().join(".claude/skills/kodosi-room/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        let mut edited = PREVIOUSLY_SHIPPED_LEGACY_ROOM_SKILL.to_vec();
        edited.extend_from_slice(b"\n# Local customization\n");
        fs::write(&skill, &edited).unwrap();

        let report = cleanup(home.path(), false).unwrap();

        assert_eq!(report.skills_removed, 0);
        assert_eq!(fs::read(skill).unwrap(), edited);
    }

    #[test]
    fn ambiguous_reference_blocks_all_mutation() {
        let home = tempfile::tempdir().unwrap();
        write_legacy_install(home.path());
        let path = home.path().join(".copilot/settings.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!(
                r#"{{"hooks":{{"preToolUse":[{{"type":"command","bash":"sh -c '{}'","powershell":"foreign"}}]}}}}"#,
                home.path().join(".kodosi/bin/kodosi-hook").display()
            ),
        )
        .unwrap();
        let before = fs::read(home.path().join(".claude/settings.json")).unwrap();

        let error = cleanup(home.path(), false).unwrap_err();

        assert!(error.to_string().contains("ambiguous"));
        assert_eq!(
            fs::read(home.path().join(".claude/settings.json")).unwrap(),
            before
        );
        assert!(home.path().join(".kodosi/bin/kodosi-hook").exists());
    }
}
