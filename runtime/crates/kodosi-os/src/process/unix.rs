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

    let connection = blocking::connection::Builder::session()
        .ok()?
        .method_timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;
    let manager: blocking::Proxy<'_> = blocking::proxy::Builder::new(&connection)
        .destination("org.freedesktop.systemd1")
        .ok()?
        .path("/org/freedesktop/systemd1")
        .ok()?
        .interface("org.freedesktop.systemd1.Manager")
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .ok()?;
    let variables: Vec<String> = manager.get_property("Environment").ok()?;
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
    use nix::sys::signal::{SigSet, Signal};
    use std::os::unix::process::ExitStatusExt as _;

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
}
