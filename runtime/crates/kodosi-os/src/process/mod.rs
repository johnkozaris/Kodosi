use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

// On Linux, executing a file fails with ETXTBSY if any process still holds a
// writable fd to it, including a child forked concurrently by another test
// thread before exec. Writing via a short-lived shell isolates the write fd.
#[cfg(all(unix, feature = "test-programs"))]
pub fn write_program(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;

    let mut shell = std::process::Command::new("/bin/sh")
        .args(["-c", r#"cat > "$1" && chmod 700 "$1""#, "sh"])
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(mut input) = shell.stdin.take() {
        input.write_all(text.as_bytes())?;
    }
    shell
        .wait()?
        .success()
        .ok_or_else(|| std::io::Error::other("the shell did not write the program"))
}

pub fn find_executable(name: &str) -> Option<PathBuf> {
    find_executable_in(name, &std::env::var_os("PATH")?)
}

pub fn find_executable_in(name: &str, paths: &OsStr) -> Option<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\']) {
        return None;
    }
    imp::find_executable(name, paths)
}

pub fn default_shell() -> PathBuf {
    imp::default_shell()
}

pub fn session_environment() -> Option<Vec<(OsString, OsString)>> {
    imp::session_environment()
}

pub fn detach(command: &mut std::process::Command) {
    imp::detach(command);
}

pub fn without_terminal(command: &mut std::process::Command) {
    imp::without_terminal(command);
}

#[cfg(unix)]
pub fn with_default_signals(command: &mut std::process::Command) {
    imp::with_default_signals(command);
}

pub fn reset_signals() {
    imp::reset_signals();
}

pub struct EndRequest(imp::EndRequest);

impl EndRequest {
    pub fn new() -> std::io::Result<Self> {
        imp::EndRequest::new().map(Self)
    }

    pub async fn received(&mut self) {
        self.0.received().await;
    }
}

#[cfg(unix)]
pub fn end_group(leader: u32, force: bool) {
    imp::end_group(leader, force);
}

/// A non-terminal process and its descendants, terminated together.
pub struct Group(imp::Group);

impl Group {
    pub fn of(pid: u32) -> std::io::Result<Self> {
        imp::Group::of(pid).map(Self)
    }

    pub fn end(&self, force: bool) {
        self.0.end(force);
    }
}

/// Creates a Windows job object that terminates its processes on last close.
#[cfg(windows)]
pub fn closing_job() -> std::io::Result<std::os::windows::io::OwnedHandle> {
    imp::closing_job()
}

pub fn is_running(pid: u32) -> bool {
    pid != 0 && imp::is_running(pid)
}

pub fn computer_name() -> Option<String> {
    imp::computer_name()
        .map(|name| name.trim().trim_end_matches(".local").to_owned())
        .filter(|name| !name.is_empty())
}

pub fn host_name() -> Option<String> {
    imp::host_name().filter(|name| !name.is_empty())
}

pub fn machine_id() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let id = std::fs::read_to_string("/etc/machine-id").ok()?;
    let id = id.trim();
    (id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())).then(|| id.to_owned())
}

pub fn allow_foreground(pid: u32) {
    imp::allow_foreground(pid);
}
