use std::path::{Path, PathBuf};

use kodosi_pty::ProcessSnapshot;

use crate::AgentKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInspection {
    pub child_pid: u32,
    pub working_dir: Option<PathBuf>,
    pub running_command: Option<String>,
    pub detected_agent: Option<String>,
}

pub fn inspect_process_from_snapshot(
    snapshot: &ProcessSnapshot,
    child_pid: u32,
    foreground_process_group: Option<u32>,
    cwd_override: Option<&Path>,
) -> ProcessInspection {
    let details = snapshot.inspect(child_pid, foreground_process_group, cwd_override);

    let argv = details.running_command;
    let detected_agent = argv
        .as_deref()
        .and_then(AgentKind::from_argv)
        .map(|kind| kind.banner().to_owned());
    let running_command = argv.map(|argv| argv.join(" "));

    ProcessInspection {
        child_pid,
        working_dir: details.working_dir,
        running_command,
        detected_agent,
    }
}
