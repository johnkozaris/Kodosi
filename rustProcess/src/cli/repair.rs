use serde::Serialize;

use crate::{Result, runtime::one_shot::OneShotApp};

use super::{args::CliRepairAction, output::OutputMode};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareTransitionResetOutput {
    preserved_evidence_paths: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CollaborationCleanupResetOutput {
    preserved_evidence_files: usize,
}

pub(in crate::cli) fn run_repair_command(
    command: &CliRepairAction,
    output: OutputMode,
) -> Result<()> {
    match command {
        CliRepairAction::ResetShareTransitions => {
            let mut app = OneShotApp::load()?;
            let preserved = app.reset_share_transition_ledger()?;
            let paths = preserved
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            if output.json {
                output.write_json(&ShareTransitionResetOutput {
                    preserved_evidence_paths: paths,
                })
            } else {
                output.write_line(format!(
                    "Reset share transition ledger; preserved {} evidence file(s): {}.",
                    paths.len(),
                    paths.join(", ")
                ))
            }
        }
        CliRepairAction::ResetCollaborationCleanup => {
            let mut app = OneShotApp::load()?;
            let preserved_evidence_files = app.reset_collaboration_cleanup_quarantine()?;
            if output.json {
                output.write_json(&CollaborationCleanupResetOutput {
                    preserved_evidence_files,
                })
            } else {
                output.write_line(format!(
                    "Reset collaboration cleanup quarantine; preserved {preserved_evidence_files} evidence file(s)."
                ))
            }
        }
        CliRepairAction::CleanupLegacyHooks { dry_run } => {
            let report =
                crate::legacy_supervision::cleanup_default_home(*dry_run)?.ok_or_else(|| {
                    crate::AppError::Unsupported {
                        reason: "could not resolve the platform home directory".to_owned(),
                    }
                })?;
            if output.json {
                output.write_json(&report)
            } else if report.changed_paths.is_empty() {
                output.write_line("No legacy Kodosi hook artifacts found.")
            } else if report.dry_run {
                output.write_line(format!(
                    "Legacy cleanup would remove {} hook entries and update: {}.",
                    report.removed_entries,
                    report.changed_paths.join(", ")
                ))
            } else {
                output.write_line(format!(
                    "Removed {} legacy hook entries; helper removed: {}; backup: {}.",
                    report.removed_entries,
                    report.helper_removed,
                    report.backup_directory.as_deref().unwrap_or("none")
                ))
            }
        }
    }
}
