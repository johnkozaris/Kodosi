use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

pub(super) fn find_executable(name: &str, paths: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(paths).find_map(|directory| {
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

#[cfg(target_os = "linux")]
pub(super) fn session_environment() -> Option<Vec<(OsString, OsString)>> {
    use zbus::{blocking, proxy::CacheProperties};

    let variables: Vec<String> = crate::session_bus::ask(|connection| {
        let manager: blocking::Proxy<'_> = blocking::proxy::Builder::new(connection)
            .destination("org.freedesktop.systemd1")
            .ok()?
            .path("/org/freedesktop/systemd1")
            .ok()?
            .interface("org.freedesktop.systemd1.Manager")
            .ok()?
            .cache_properties(CacheProperties::No)
            .build()
            .ok()?;
        manager.get_property("Environment").ok()
    })?;
    Some(
        variables
            .iter()
            .filter_map(|variable| variable.split_once('='))
            .map(|(name, value)| (name.into(), value.into()))
            .collect(),
    )
}

#[cfg(not(target_os = "linux"))]
pub(super) const fn session_environment() -> Option<Vec<(OsString, OsString)>> {
    None
}

pub(super) fn detach(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt as _;
    command.process_group(0);
}

#[allow(unsafe_code)]
pub(super) fn without_terminal(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt as _;
    unsafe {
        command.pre_exec(|| rustix::process::setsid().map(drop).map_err(Into::into));
    }
    with_default_signals(command);
}

#[allow(unsafe_code)]
pub(super) fn with_default_signals(command: &mut std::process::Command) {
    use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, sigaction};
    use std::os::unix::process::CommandExt as _;
    unsafe {
        command.pre_exec(|| {
            let default = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());
            for signal in Signal::iterator()
                .filter(|signal| !matches!(signal, Signal::SIGKILL | Signal::SIGSTOP))
            {
                sigaction(signal, &default)?;
            }
            SigSet::empty().thread_set_mask()?;
            Ok(())
        });
    }
}

#[allow(unsafe_code)]
pub(super) fn reset_signals() {
    use nix::sys::signal::{SigHandler, SigSet, Signal, signal};
    let _ = SigSet::empty().thread_set_mask();
    let _ = unsafe { signal(Signal::SIGPIPE, SigHandler::SigIgn) };
}

pub(super) fn end_group(leader: u32, force: bool) {
    let Some(leader) = i32::try_from(leader)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    else {
        return;
    };
    let signal = if force {
        rustix::process::Signal::KILL
    } else {
        rustix::process::Signal::TERM
    };
    let _ = rustix::process::kill_process_group(leader, signal);
}

pub(super) fn end_process(pid: u32) {
    if let Some(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
    }
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

pub(super) fn host_name() -> Option<String> {
    rustix::system::uname()
        .nodename()
        .to_str()
        .ok()
        .map(str::to_owned)
}

pub(super) const fn allow_foreground(_pid: u32) {}

#[cfg(test)]
mod tests {
    use nix::sys::signal::{SigHandler, SigSet, Signal, signal};
    use std::{io::Write as _, os::unix::process::ExitStatusExt as _};

    #[test]
    fn a_program_started_without_a_terminal_ends_on_a_signal_its_host_blocks() {
        let status = std::thread::spawn(|| {
            SigSet::from(Signal::SIGTERM).thread_block().unwrap();
            let mut program = std::process::Command::new("/bin/sh");
            program.args(["-c", "kill -TERM $$; exec sleep 5"]);
            super::without_terminal(&mut program);
            program.status().unwrap()
        })
        .join()
        .unwrap();
        assert_eq!(status.signal(), Some(Signal::SIGTERM as i32));
    }

    #[test]
    #[allow(unsafe_code)]
    fn a_program_started_with_blocked_signals_hears_them_and_survives_a_closed_pipe() {
        const NAME: &str = "process::unix::tests::a_program_started_with_blocked_signals_hears_them_and_survives_a_closed_pipe";
        const CHILD: &str = "KODOSI_TEST_BLOCKED_SIGNALS";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1"])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            return;
        }
        unsafe { signal(Signal::SIGPIPE, SigHandler::SigDfl) }.unwrap();
        SigSet::from_iter([Signal::SIGCHLD, Signal::SIGTERM])
            .thread_block()
            .unwrap();
        super::reset_signals();
        let mask = SigSet::thread_get_mask().unwrap();
        assert!(!mask.contains(Signal::SIGCHLD) && !mask.contains(Signal::SIGTERM));
        let (reader, mut writer) = std::io::pipe().unwrap();
        drop(reader);
        assert_eq!(
            writer.write_all(b"x").unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
    }
}
