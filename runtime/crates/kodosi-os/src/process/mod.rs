use std::path::PathBuf;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;
#[cfg(windows)]
pub(crate) use windows::executable_extensions;

/// Finds the program `name` on `PATH`, as a shell would.
pub fn find_executable(name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\']) {
        return None;
    }
    imp::find_executable(name)
}

/// The shell a new terminal runs when the user configured none.
pub fn default_shell() -> PathBuf {
    imp::default_shell()
}

/// Starts `command` outside the caller's console and signal group, so it
/// outlives the terminal that launched it.
pub fn detach(command: &mut std::process::Command) {
    imp::detach(command);
}

/// Whether process `pid` may still be running. Unknown counts as running.
pub fn is_running(pid: u32) -> bool {
    pid != 0 && imp::is_running(pid)
}

/// The name the user gave this computer.
pub fn computer_name() -> Option<String> {
    imp::computer_name()
        .map(|name| name.trim().trim_end_matches(".local").to_owned())
        .filter(|name| !name.is_empty())
}

/// Lets process `pid` bring its window to the front once.
pub fn allow_foreground(pid: u32) {
    imp::allow_foreground(pid);
}
