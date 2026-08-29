use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use sysinfo::{Process, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessDetails {
    pub working_dir: Option<PathBuf>,
    pub running_command: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessTarget {
    pub child_pid: u32,
    pub foreground_process_group: Option<u32>,
}

impl ProcessTarget {
    #[must_use]
    pub const fn new(child_pid: u32, foreground_process_group: Option<u32>) -> Self {
        Self {
            child_pid,
            foreground_process_group,
        }
    }

    fn requested_pids(self) -> Vec<sysinfo::Pid> {
        let mut pids = vec![sysinfo::Pid::from_u32(self.child_pid)];
        if let Some(foreground) = self.foreground_process_group
            && foreground != self.child_pid
        {
            pids.push(sysinfo::Pid::from_u32(foreground));
        }
        pids
    }
}

#[derive(Debug, Clone)]
struct ProcessRecord {
    parent_pid: Option<u32>,
    working_dir: Option<PathBuf>,
    command: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessSnapshot {
    processes: HashMap<u32, ProcessRecord>,
}

fn metadata_refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cwd(UpdateKind::Always)
        .with_cmd(UpdateKind::Always)
}

fn record_of(process: &Process) -> ProcessRecord {
    ProcessRecord {
        parent_pid: process.parent().map(sysinfo::Pid::as_u32),
        working_dir: process.cwd().map(Path::to_path_buf),
        command: process
            .cmd()
            .iter()
            .map(|segment| segment.to_string_lossy().into_owned())
            .collect(),
    }
}

impl ProcessSnapshot {
    #[must_use]
    pub fn capture_for(target: ProcessTarget) -> Self {
        let mut system = System::new();
        let requested = target.requested_pids();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&requested),
            false,
            metadata_refresh_kind(),
        );

        let mut processes: HashMap<u32, ProcessRecord> = requested
            .iter()
            .filter_map(|pid| {
                system
                    .process(*pid)
                    .map(|process| (pid.as_u32(), record_of(process)))
            })
            .collect();

        if needs_child_scan(&processes, target) {
            let mut scan = System::new();
            scan.refresh_processes_specifics(
                ProcessesToUpdate::All,
                false,
                metadata_refresh_kind(),
            );
            let descendants = descendant_records(&scan, target.child_pid);
            processes.extend(descendants);
            if let Some(process) = scan.process(sysinfo::Pid::from_u32(target.child_pid)) {
                processes
                    .entry(target.child_pid)
                    .or_insert_with(|| record_of(process));
            }
        }

        Self { processes }
    }

    #[must_use]
    pub fn inspect(
        &self,
        child_pid: u32,
        foreground_process_group: Option<u32>,
        cwd_override: Option<&Path>,
    ) -> ProcessDetails {
        let child = self.processes.get(&child_pid);
        let working_dir = cwd_override
            .map(Path::to_path_buf)
            .or_else(|| child.and_then(|process| process.working_dir.clone()));
        let command = self
            .command_process(child_pid, foreground_process_group)
            .or(child)
            .map(|process| process.command.clone())
            .filter(|command| !command.is_empty());

        ProcessDetails {
            working_dir,
            running_command: command,
        }
    }

    #[cfg(test)]
    fn tracked_pids(&self) -> Vec<u32> {
        let mut pids: Vec<u32> = self.processes.keys().copied().collect();
        pids.sort_unstable();
        pids
    }

    fn command_process(
        &self,
        child_pid: u32,
        foreground_process_group: Option<u32>,
    ) -> Option<&ProcessRecord> {
        if let Some(foreground_pid) = foreground_process_group
            && let Some(process) = self.processes.get(&foreground_pid)
            && !process.command.is_empty()
        {
            return Some(process);
        }

        self.processes
            .iter()
            .filter(|(_, process)| is_descendant_of(&self.processes, process, child_pid))
            .filter(|(_, process)| !process.command.is_empty())
            .max_by_key(|(pid, process)| {
                (descendant_depth(&self.processes, process, child_pid), **pid)
            })
            .map(|(_, process)| process)
    }
}

fn descendant_depth(
    processes: &HashMap<u32, ProcessRecord>,
    process: &ProcessRecord,
    ancestor_pid: u32,
) -> usize {
    let mut parent = process.parent_pid;
    let mut remaining = processes.len();
    let mut depth = 0;
    while let Some(pid) = parent
        && remaining > 0
    {
        depth += 1;
        if pid == ancestor_pid {
            return depth;
        }
        parent = processes.get(&pid).and_then(|record| record.parent_pid);
        remaining -= 1;
    }
    0
}

fn is_descendant_of(
    processes: &HashMap<u32, ProcessRecord>,
    process: &ProcessRecord,
    ancestor_pid: u32,
) -> bool {
    let mut parent = process.parent_pid;
    let mut remaining = processes.len();
    while let Some(pid) = parent
        && remaining > 0
    {
        if pid == ancestor_pid {
            return true;
        }
        parent = processes.get(&pid).and_then(|record| record.parent_pid);
        remaining -= 1;
    }
    false
}

fn descendant_records(system: &System, child_pid: u32) -> HashMap<u32, ProcessRecord> {
    let parents: HashMap<u32, Option<u32>> = system
        .processes()
        .iter()
        .map(|(pid, process)| (pid.as_u32(), process.parent().map(sysinfo::Pid::as_u32)))
        .collect();
    system
        .processes()
        .iter()
        .filter(|(pid, _)| is_pid_descendant_of(&parents, pid.as_u32(), child_pid))
        .map(|(pid, process)| (pid.as_u32(), record_of(process)))
        .collect()
}

fn is_pid_descendant_of(parents: &HashMap<u32, Option<u32>>, pid: u32, ancestor_pid: u32) -> bool {
    let mut current = parents.get(&pid).copied().flatten();
    let mut remaining = parents.len();
    while let Some(parent) = current
        && remaining > 0
    {
        if parent == ancestor_pid {
            return true;
        }
        current = parents.get(&parent).copied().flatten();
        remaining -= 1;
    }
    false
}

fn needs_child_scan(processes: &HashMap<u32, ProcessRecord>, target: ProcessTarget) -> bool {
    let Some(foreground_pid) = target.foreground_process_group else {
        return true;
    };
    processes
        .get(&foreground_pid)
        .is_none_or(|process| process.command.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{ProcessRecord, ProcessSnapshot, ProcessTarget, needs_child_scan};
    use std::{collections::HashMap, path::PathBuf};

    fn record(parent_pid: Option<u32>, cwd: Option<&str>, command: &[&str]) -> ProcessRecord {
        ProcessRecord {
            parent_pid,
            working_dir: cwd.map(PathBuf::from),
            command: command.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn foreground_group_leader_wins_without_scanning_per_session() {
        let snapshot = ProcessSnapshot {
            processes: HashMap::from([
                (100, record(None, Some("/repo"), &["zsh"])),
                (200, record(Some(100), None, &["claude", "--resume"])),
                (201, record(Some(100), None, &["node", "helper.js"])),
            ]),
        };

        let details = snapshot.inspect(100, Some(200), None);

        assert_eq!(details.working_dir, Some(PathBuf::from("/repo")));
        assert_eq!(
            details.running_command,
            Some(vec!["claude".to_owned(), "--resume".to_owned()])
        );
    }

    #[test]
    fn single_descendant_is_cross_platform_fallback() {
        let snapshot = ProcessSnapshot {
            processes: HashMap::from([
                (100, record(None, Some("/repo"), &["pwsh"])),
                (200, record(Some(100), None, &["copilot"])),
            ]),
        };

        let details = snapshot.inspect(100, None, Some(PathBuf::from("/osc7")).as_deref());

        assert_eq!(details.working_dir, Some(PathBuf::from("/osc7")));
        assert_eq!(details.running_command, Some(vec!["copilot".to_owned()]));
    }

    #[test]
    fn multiple_descendants_choose_deepest_then_highest_pid_deterministically() {
        let snapshot = ProcessSnapshot {
            processes: HashMap::from([
                (100, record(None, Some("/repo"), &["pwsh"])),
                (200, record(Some(100), None, &["sleep"])),
                (205, record(Some(100), None, &["node", "helper.js"])),
                (210, record(Some(205), None, &["copilot"])),
                (211, record(Some(205), None, &["claude"])),
            ]),
        };

        let details = snapshot.inspect(100, None, None);

        assert_eq!(details.running_command, Some(vec!["claude".to_owned()]));
    }

    #[test]
    fn requested_pids_deduplicate_a_self_led_foreground_group() {
        assert_eq!(
            ProcessTarget::new(100, Some(100)).requested_pids(),
            vec![sysinfo::Pid::from_u32(100)]
        );
        assert_eq!(
            ProcessTarget::new(100, Some(200)).requested_pids(),
            vec![sysinfo::Pid::from_u32(100), sysinfo::Pid::from_u32(200)]
        );
        assert_eq!(
            ProcessTarget::new(100, None).requested_pids(),
            vec![sysinfo::Pid::from_u32(100)]
        );
    }

    #[test]
    fn child_scan_is_required_only_when_the_foreground_command_is_unknown() {
        let resolved = HashMap::from([(200, record(Some(100), None, &["claude"]))]);
        assert!(!needs_child_scan(
            &resolved,
            ProcessTarget::new(100, Some(200))
        ));

        assert!(needs_child_scan(&resolved, ProcessTarget::new(100, None)));

        assert!(needs_child_scan(
            &HashMap::new(),
            ProcessTarget::new(100, Some(200))
        ));

        let empty = HashMap::from([(200, record(Some(100), None, &[]))]);
        assert!(needs_child_scan(&empty, ProcessTarget::new(100, Some(200))));
    }

    #[test]
    fn capture_records_only_the_requested_pids() {
        let self_pid = std::process::id();
        let snapshot = ProcessSnapshot::capture_for(ProcessTarget::new(self_pid, Some(self_pid)));

        assert_eq!(snapshot.tracked_pids(), vec![self_pid]);
        let details = snapshot.inspect(self_pid, Some(self_pid), None);
        assert!(details.running_command.is_some_and(|argv| !argv.is_empty()));
    }
}
