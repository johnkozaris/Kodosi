use std::path::{Path, PathBuf};

pub(super) fn find_executable(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).find_map(|directory| {
        let path = directory.join(name);
        crate::fs::is_executable(&path)
            .then(|| crate::fs::canonicalize(&path).ok())
            .flatten()
    })
}

pub(super) fn default_shell() -> PathBuf {
    std::env::var_os("SHELL")
        .filter(|shell| Path::new(shell).is_absolute())
        .map_or_else(|| PathBuf::from("/bin/sh"), PathBuf::from)
}

pub(super) fn detach(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt as _;
    command.process_group(0);
}

pub(super) fn is_running(pid: u32) -> bool {
    let Some(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    else {
        return false;
    };
    rustix::process::test_kill_process(pid) != Err(rustix::io::Errno::SRCH)
}

pub(super) fn computer_name() -> Option<String> {
    let (program, arguments): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("/usr/sbin/scutil", &["--get", "ComputerName"])
    } else {
        ("hostname", &[])
    };
    let output = std::process::Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    String::from_utf8(output.stdout).ok()
}

pub(super) const fn allow_foreground(_pid: u32) {}
