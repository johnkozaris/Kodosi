use std::{
    env,
    fs::File,
    io,
    os::fd::{AsFd, AsRawFd, OwnedFd},
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::Duration,
};

use crate::{KodosiError, Result};
use nix::{
    errno::Errno,
    pty::openpty,
    sys::signal::{Signal, killpg},
    unistd::{self, Pid},
};
use rustix::{
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
    io::{FdFlags, fcntl_setfd},
    termios::{LocalModes, tcgetattr, tcgetpgrp, tcsetwinsize},
};
use tokio::{
    io::{Interest, Ready, unix::AsyncFd},
    process::{Child, Command},
};

use super::{ShutdownStage, Transfer, WaitOutcome, program_identity, validate_working_dir};

static OPEN_PTY: Mutex<()> = Mutex::new(());

#[derive(Debug)]
pub struct PtyReader {
    pending: Option<File>,
    async_fd: Option<AsyncFd<File>>,
}

impl PtyReader {
    fn new(fd: OwnedFd) -> io::Result<Self> {
        fcntl_setfl(&fd, fcntl_getfl(&fd)? | OFlags::NONBLOCK)?;

        let file = File::from(fd);
        Ok(Self {
            pending: Some(file),
            async_fd: None,
        })
    }

    fn get_async_fd(&mut self) -> io::Result<&mut AsyncFd<File>> {
        if self.async_fd.is_none() {
            let file = self
                .pending
                .take()
                .ok_or_else(|| io::Error::other("missing pending PTY reader file"))?;
            self.async_fd = Some(unsafe { AsyncFd::register(file) }?);
        }
        self.async_fd
            .as_mut()
            .ok_or_else(|| io::Error::other("PTY async reader failed to initialize"))
    }

    pub const fn write_in_flight(&self) -> bool {
        false
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let async_fd = self.get_async_fd()?;
        loop {
            let mut guard = async_fd.readable().await?;
            match guard.try_io(|inner| Ok(rustix::io::read(inner.get_ref(), &mut *buf)?)) {
                Ok(Ok(bytes_read)) => return Ok(bytes_read),
                Ok(Err(error)) => return Err(error),
                Err(_would_block) => continue,
            }
        }
    }

    pub async fn transfer(
        &mut self,
        read: &mut [u8],
        write: Option<&[u8]>,
    ) -> io::Result<Transfer> {
        let async_fd = self.get_async_fd()?;
        let interest = if write.is_some() {
            Interest::READABLE | Interest::WRITABLE
        } else {
            Interest::READABLE
        };
        loop {
            let mut guard = async_fd.ready(interest).await?;
            let ready = guard.ready();
            if let Some(bytes) = write
                && ready.is_writable()
            {
                match rustix::io::write(guard.get_inner(), bytes) {
                    Ok(0) | Err(rustix::io::Errno::AGAIN) => {
                        guard.clear_ready_matching(Ready::WRITABLE);
                    }
                    Ok(count) => return Ok(Transfer::Wrote(count)),
                    Err(rustix::io::Errno::INTR) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            if ready.is_readable() {
                match rustix::io::read(guard.get_inner(), &mut *read) {
                    Ok(count) => return Ok(Transfer::Read(count)),
                    Err(rustix::io::Errno::AGAIN) => guard.clear_ready_matching(Ready::READABLE),
                    Err(rustix::io::Errno::INTR) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct KodosiPty {
    master: OwnedFd,
    child_pid: u32,
    child: Child,
}

impl KodosiPty {
    pub fn spawn_program(
        program: &Path,
        arguments: &[String],
        working_dir: Option<&str>,
        rows: u16,
        cols: u16,
        environment: &[(&str, &str)],
        prompt_marks: Option<&Path>,
    ) -> Result<(Self, PtyReader)> {
        let working_dir = validate_working_dir(working_dir)?;
        let resolved_program = resolve_program(program, working_dir).ok_or_else(|| {
            KodosiError::Spawn(format!("command not found: {}", program.display()))
        })?;

        let openpty_result = {
            let _serial = OPEN_PTY.lock().unwrap_or_else(PoisonError::into_inner);
            openpty(None, None).map_err(open_failure)?
        };
        set_terminal_size(&openpty_result.master, cols, rows, None, None)?;
        let slave_raw = openpty_result.slave.as_raw_fd();
        for descriptor in [&openpty_result.master, &openpty_result.slave] {
            fcntl_setfd(descriptor, FdFlags::CLOEXEC).map_err(io::Error::from)?;
        }
        let reader = PtyReader::new(openpty_result.master.try_clone()?)?;

        let mut command = Command::new(&resolved_program);
        command.args(arguments);
        let login_shell = arguments
            .is_empty()
            .then(|| login_shell_name(&resolved_program))
            .flatten();
        if let Some(shell) = login_shell {
            command.arg0(format!("-{shell}"));
            command.env("SHELL", &resolved_program);
        }
        if let Some(directory) = working_dir {
            command.current_dir(directory);
        }
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("TERM_PROGRAM", crate::TERMINAL_PROGRAM);
        command.env("TERM_PROGRAM_VERSION", crate::GHOSTTY_VERSION);
        command.env_remove("TERMINFO");
        if env::var_os("LANG").is_none() && env::var_os("LC_ALL").is_none() {
            command.env("LANG", "en_US.UTF-8");
        }

        for (key, value) in environment {
            command.env(key, value);
        }
        if let (Some(shell), Some(folder)) = (login_shell, prompt_marks) {
            super::prompt_marks::add(shell, &resolved_program, folder, environment, &mut command);
        }
        command.kill_on_drop(true);

        unsafe {
            command.pre_exec(move || {
                if libc::login_tty(slave_raw) != 0 {
                    return Err(io::Error::last_os_error());
                }
                close_fds::set_fds_cloexec(3, &[]);
                Ok(())
            });
        }
        kodosi_os::process::with_default_signals(command.as_std_mut());

        let child = command.spawn()?;
        let child_pid = child.id().ok_or_else(|| {
            KodosiError::Spawn("spawned shell did not report a process id".to_owned())
        })?;

        drop(openpty_result.slave);

        Ok((
            Self {
                master: openpty_result.master,
                child_pid,
                child,
            },
            reader,
        ))
    }

    pub fn working_directory(&self) -> Option<PathBuf> {
        let pid = foreground_process_group(&self.master).unwrap_or(self.child_pid);
        process_directory(pid).or_else(|| process_directory(self.child_pid))
    }

    pub fn foreground_process_group(&self) -> Option<u32> {
        foreground_process_group(&self.master)
    }

    pub fn foreground_is_child(&self) -> bool {
        foreground_process_group(&self.master) == Some(self.child_pid)
    }

    pub fn reads_keys(&self) -> bool {
        tcgetattr(&self.master)
            .is_ok_and(|attributes| !attributes.local_modes.contains(LocalModes::ICANON))
    }

    pub fn foreground_program(&self) -> Option<String> {
        let pid = foreground_process_group(&self.master)?;
        let arguments = process_arguments(pid)?;
        program_identity(&arguments).map(str::to_owned)
    }

    pub fn foreground_variable(&self, name: &str) -> Option<String> {
        let pid = foreground_process_group(&self.master)?;
        variable(&process_environment(pid)?, name)
    }

    pub fn foreground_arguments(&self) -> Vec<String> {
        foreground_process_group(&self.master)
            .and_then(process_arguments)
            .unwrap_or_default()
    }

    pub fn resize(
        &self,
        rows: u16,
        cols: u16,
        width_pixels: Option<u16>,
        height_pixels: Option<u16>,
    ) -> Result<()> {
        set_terminal_size(&self.master, cols, rows, width_pixels, height_pixels)
    }

    pub fn request_shutdown(&mut self, stage: ShutdownStage) -> Result<()> {
        let signal = signal_for_shutdown_stage(stage);
        if stage == ShutdownStage::Interrupt {
            return self.send_signal(signal);
        }
        signal_processes(session_processes(self.child_pid)?, |process| {
            if process_belongs_to_session(process, self.child_pid) {
                Ok(nix::sys::signal::kill(Pid::from_raw(process), signal)?)
            } else {
                Ok(())
            }
        })?;
        Ok(())
    }

    pub async fn wait_within(&mut self, grace: Duration) -> Result<WaitOutcome> {
        match tokio::time::timeout(grace, async {
            self.child.wait().await?;
            while !session_processes(self.child_pid)?.is_empty() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok(())
        })
        .await
        {
            Ok(result) => result.map(|()| WaitOutcome::Reaped),
            Err(_) => Ok(WaitOutcome::TimedOut),
        }
    }

    fn signal_target(&self) -> Option<Pid> {
        let owner_group = unistd::getpgrp();
        let owner_pid = unistd::getpid();
        let shell_group = Pid::from_raw(self.child_pid as i32);
        foreground_process_group(&self.master)
            .and_then(|pgid| i32::try_from(pgid).ok())
            .map(Pid::from_raw)
            .filter(|group| process_group_belongs_to_session(*group, shell_group))
            .or_else(|| {
                process_group_belongs_to_session(shell_group, shell_group).then_some(shell_group)
            })
            .filter(|group| *group != owner_group && *group != owner_pid)
    }

    fn send_signal(&self, signal: Signal) -> Result<()> {
        let shell_group = Pid::from_raw(self.child_pid as i32);
        let Some(target) = self.signal_target() else {
            return Ok(());
        };

        match signal_process_group(target, signal) {
            Err(error) if target != shell_group && is_missing_process_group(&error) => {
                if process_group_belongs_to_session(shell_group, shell_group) {
                    signal_process_group(shell_group, signal)
                } else {
                    Ok(())
                }
            }
            result => result,
        }
    }
}

/// The answer of `/dev/ptmx` when every pseudo-terminal of the computer is in use: macOS stops at
/// `kern.tty.ptmx_max`, Linux at `kernel.pty.max`.
#[cfg(target_os = "macos")]
const NO_FREE_PTY: Errno = Errno::ENXIO;
#[cfg(not(target_os = "macos"))]
const NO_FREE_PTY: Errno = Errno::ENOSPC;

fn open_failure(error: Errno) -> KodosiError {
    if error == NO_FREE_PTY {
        KodosiError::NoFreePty
    } else {
        io::Error::from(error).into()
    }
}

fn signal_processes(
    processes: Vec<i32>,
    mut signal: impl FnMut(i32) -> io::Result<()>,
) -> io::Result<()> {
    let mut failure = None;
    for process in processes {
        if let Err(error) = signal(process)
            && error.raw_os_error() != Some(libc::ESRCH)
        {
            failure.get_or_insert(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

fn process_belongs_to_session(process: i32, session: u32) -> bool {
    process > 0
        && process != unistd::getpid().as_raw()
        && unistd::getsid(Some(Pid::from_raw(process)))
            .is_ok_and(|sid| u32::try_from(sid.as_raw()) == Ok(session))
}

#[cfg(target_os = "linux")]
fn session_processes(session: u32) -> io::Result<Vec<i32>> {
    let mut processes = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse().ok())
        else {
            continue;
        };
        if process_belongs_to_session(pid, session)
            && process_is_live(std::fs::read_to_string(entry.path().join("stat")))?
        {
            processes.push(pid);
        }
    }
    Ok(processes)
}

#[cfg(target_os = "linux")]
fn process_is_live(stat: io::Result<String>) -> io::Result<bool> {
    match stat {
        Ok(stat) => {
            let state = stat
                .rsplit_once(')')
                .and_then(|(_, fields)| fields.split_whitespace().next());
            Ok(!matches!(state, Some("Z" | "X")))
        }
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(libc::ESRCH) =>
        {
            Ok(false)
        }
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Ok(true),
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "macos")]
fn session_processes(session: u32) -> io::Result<Vec<i32>> {
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut capacity = usize::try_from(count).unwrap_or(0).saturating_add(256);
    let pids = loop {
        let mut pids = vec![0_i32; capacity];
        let bytes = i32::try_from(std::mem::size_of_val(pids.as_slice()))
            .map_err(|_| io::Error::other("process list exceeds its native bound"))?;
        let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        let count =
            usize::try_from(count).map_err(|_| io::Error::other("invalid process count"))?;
        if count < capacity {
            pids.truncate(count);
            break pids;
        }
        capacity = capacity
            .checked_mul(2)
            .filter(|value| *value <= 1_048_576)
            .ok_or_else(|| io::Error::other("process list did not stabilize"))?;
    };
    let mut processes = Vec::new();
    for pid in pids {
        if !process_belongs_to_session(pid, session) {
            continue;
        }
        let mut info = libc::proc_bsdinfo::default();
        let size = i32::try_from(std::mem::size_of_val(&info))
            .map_err(|_| io::Error::other("invalid native process info size"))?;
        let read = unsafe {
            libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size)
        };
        if read != size {
            if process_belongs_to_session(pid, session) {
                processes.push(pid);
            }
            continue;
        }
        if info.pbi_status != libc::SZOMB {
            processes.push(pid);
        }
    }
    Ok(processes)
}

fn process_group_belongs_to_session(group: Pid, session: Pid) -> bool {
    group.as_raw() > 0 && unistd::getsid(Some(group)).is_ok_and(|sid| sid == session)
}

fn signal_process_group(group: Pid, signal: Signal) -> Result<()> {
    match killpg(group, signal) {
        Ok(()) => Ok(()),
        Err(error) => Err(KodosiError::Io(error.into())),
    }
}

fn is_missing_process_group(error: &KodosiError) -> bool {
    matches!(error, KodosiError::Io(error) if error.raw_os_error() == Some(libc::ESRCH))
}

fn signal_for_shutdown_stage(stage: ShutdownStage) -> Signal {
    match stage {
        ShutdownStage::Interrupt => Signal::SIGINT,
        ShutdownStage::Hangup => Signal::SIGHUP,
        ShutdownStage::Terminate => Signal::SIGTERM,
        ShutdownStage::Force => Signal::SIGKILL,
    }
}

impl Drop for KodosiPty {
    fn drop(&mut self) {
        let _ = self.request_shutdown(ShutdownStage::Force);
    }
}

fn set_terminal_size(
    terminal: impl AsFd,
    columns: u16,
    rows: u16,
    width_in_pixels: Option<u16>,
    height_in_pixels: Option<u16>,
) -> Result<()> {
    let size = rustix::termios::Winsize {
        ws_col: columns,
        ws_row: rows,
        ws_xpixel: width_in_pixels.unwrap_or(0),
        ws_ypixel: height_in_pixels.unwrap_or(0),
    };
    Ok(tcsetwinsize(terminal, size).map_err(io::Error::from)?)
}

const LOGIN_SHELLS: [&str; 8] = ["bash", "csh", "dash", "fish", "ksh", "sh", "tcsh", "zsh"];

fn login_shell_name(program: &Path) -> Option<&str> {
    let name = program.file_name()?.to_str()?;
    LOGIN_SHELLS.contains(&name).then_some(name)
}

fn find_executable(candidate: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;

    let metadata = candidate.metadata().ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .then(|| candidate.to_path_buf())
}

fn resolve_program(program: &Path, working_dir: Option<&Path>) -> Option<PathBuf> {
    if program.is_absolute() {
        return find_executable(program);
    }
    if program.components().count() > 1 {
        return find_executable(
            &working_dir.map_or_else(|| program.to_owned(), |cwd| cwd.join(program)),
        );
    }
    let paths = env::var_os("PATH")?;
    for path in env::split_paths(&paths) {
        if let Some(resolved) = find_executable(
            &working_dir.map_or_else(|| path.join(program), |cwd| cwd.join(&path).join(program)),
        ) {
            return Some(resolved);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn process_directory(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

#[cfg(target_os = "macos")]
fn process_directory(pid: u32) -> Option<PathBuf> {
    let mut info = libc::proc_vnodepathinfo::default();
    let size = i32::try_from(std::mem::size_of_val(&info)).ok()?;
    let result = unsafe {
        libc::proc_pidinfo(
            i32::try_from(pid).ok()?,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            (&raw mut info).cast(),
            size,
        )
    };
    if result != size {
        return None;
    }
    vnode_path(&info.pvi_cdir)
}

#[cfg(target_os = "macos")]
fn vnode_path(vnode: &libc::vnode_info_path) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt as _;
    let bytes = vnode
        .vip_path
        .iter()
        .flatten()
        .take_while(|byte| **byte != 0)
        .map(|byte| byte.to_ne_bytes()[0])
        .collect::<Vec<_>>();
    (!bytes.is_empty()).then(|| PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

const MAX_OPEN_FILES: usize = 16_384;

#[cfg(target_os = "macos")]
const MAX_GROUP_PROCESSES: usize = 1024;

#[cfg(target_os = "macos")]
const PROC_PIDFDVNODEPATHINFO: i32 = 2;

/// `struct vnode_fdinfowithpath` of `sys/proc_info.h`, after a file part that Kodosi does not
/// read.
#[cfg(target_os = "macos")]
#[derive(Default)]
#[repr(C)]
struct VnodeFileInfo {
    _file: [u64; 3],
    vnode: libc::vnode_info_path,
}

#[cfg(target_os = "macos")]
pub fn open_files(group: u32) -> Option<Vec<PathBuf>> {
    let mut pids = vec![0_i32; MAX_GROUP_PROCESSES];
    let bytes = i32::try_from(std::mem::size_of_val(pids.as_slice())).ok()?;
    let count = unsafe {
        libc::proc_listpgrppids(i32::try_from(group).ok()?, pids.as_mut_ptr().cast(), bytes)
    };
    pids.truncate(usize::try_from(count).ok()?);
    Some(
        pids.into_iter()
            .flat_map(process_files)
            .take(MAX_OPEN_FILES)
            .collect(),
    )
}

#[cfg(target_os = "macos")]
fn process_files(pid: i32) -> Vec<PathBuf> {
    let entry = std::mem::size_of::<libc::proc_fdinfo>();
    let needed =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    let entries = usize::try_from(needed).unwrap_or(0) / entry;
    let mut descriptors = vec![libc::proc_fdinfo::default(); entries.min(MAX_OPEN_FILES)];
    let Ok(bytes) = i32::try_from(std::mem::size_of_val(descriptors.as_slice())) else {
        return Vec::new();
    };
    let filled = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            descriptors.as_mut_ptr().cast(),
            bytes,
        )
    };
    descriptors.truncate(usize::try_from(filled).unwrap_or(0) / entry);
    descriptors
        .iter()
        .filter(|descriptor| i32::try_from(descriptor.proc_fdtype) == Ok(libc::PROX_FDTYPE_VNODE))
        .filter_map(|descriptor| {
            let mut info = VnodeFileInfo::default();
            let size = i32::try_from(std::mem::size_of_val(&info)).ok()?;
            let read = unsafe {
                libc::proc_pidfdinfo(
                    pid,
                    descriptor.proc_fd,
                    PROC_PIDFDVNODEPATHINFO,
                    (&raw mut info).cast(),
                    size,
                )
            };
            (read == size).then(|| vnode_path(&info.vnode)).flatten()
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub fn open_files(group: u32) -> Option<Vec<PathBuf>> {
    let processes = std::fs::read_dir("/proc").ok()?;
    Some(
        processes
            .flatten()
            .filter(|process| {
                process
                    .file_name()
                    .to_str()
                    .and_then(|name| name.parse().ok())
                    .and_then(process_group)
                    == Some(group)
            })
            .filter_map(|process| std::fs::read_dir(process.path().join("fd")).ok())
            .flat_map(|descriptors| {
                descriptors
                    .flatten()
                    .filter_map(|descriptor| std::fs::read_link(descriptor.path()).ok())
            })
            .filter(|path| path.is_absolute())
            .take(MAX_OPEN_FILES)
            .collect(),
    )
}

#[cfg(target_os = "linux")]
fn process_arguments(pid: u32) -> Option<Vec<String>> {
    Some(strings(
        &std::fs::read(format!("/proc/{pid}/cmdline")).ok()?,
    ))
}

#[cfg(target_os = "linux")]
fn process_environment(pid: u32) -> Option<Vec<String>> {
    Some(strings(
        &std::fs::read(format!("/proc/{pid}/environ")).ok()?,
    ))
}

#[cfg(target_os = "linux")]
fn strings(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

#[cfg(target_os = "macos")]
fn process_arguments(pid: u32) -> Option<Vec<String>> {
    process_strings(pid).map(|(arguments, _)| arguments)
}

#[cfg(target_os = "macos")]
fn process_environment(pid: u32) -> Option<Vec<String>> {
    process_strings(pid).map(|(_, environment)| environment)
}

#[cfg(target_os = "macos")]
fn process_strings(pid: u32) -> Option<(Vec<String>, Vec<String>)> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
    let mut bytes = vec![0_u8; 256 * 1024];
    let mut length = bytes.len();
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &raw mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || length < 4 {
        return None;
    }
    bytes.truncate(length);
    let count = i32::from_ne_bytes(bytes[..4].try_into().ok()?);
    if !(1..=4096).contains(&count) {
        return None;
    }
    let mut cursor = 4 + bytes[4..].iter().position(|byte| *byte == 0)?;
    while bytes.get(cursor) == Some(&0) {
        cursor += 1;
    }
    let mut arguments = Vec::new();
    for _ in 0..count {
        let end = cursor + bytes.get(cursor..)?.iter().position(|byte| *byte == 0)?;
        arguments.push(String::from_utf8_lossy(&bytes[cursor..end]).into_owned());
        cursor = end + 1;
    }
    let mut environment = Vec::new();
    while let Some(rest) = bytes.get(cursor..) {
        let length = rest
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(rest.len());
        if length == 0 {
            break;
        }
        environment.push(String::from_utf8_lossy(&rest[..length]).into_owned());
        cursor += length + 1;
    }
    Some((arguments, environment))
}

fn variable(environment: &[String], name: &str) -> Option<String> {
    environment
        .iter()
        .find_map(|entry| entry.strip_prefix(name)?.strip_prefix('='))
        .map(str::to_owned)
}

/// What a running process of this user started with.
pub struct ProcessStart {
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub variables: Vec<(String, String)>,
    pub directory: PathBuf,
}

pub fn process_start(pid: u32) -> Option<ProcessStart> {
    Some(ProcessStart {
        program: process_program(pid)?,
        arguments: process_arguments(pid)?,
        variables: process_environment(pid)?
            .iter()
            .filter_map(|entry| entry.split_once('='))
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect(),
        directory: process_directory(pid)?,
    })
}

#[cfg(target_os = "linux")]
fn process_program(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

#[cfg(target_os = "macos")]
fn process_program(pid: u32) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt as _;
    let mut path = vec![0_u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let length = unsafe {
        libc::proc_pidpath(
            i32::try_from(pid).ok()?,
            path.as_mut_ptr().cast(),
            u32::try_from(path.len()).ok()?,
        )
    };
    path.truncate(usize::try_from(length).ok().filter(|length| *length > 0)?);
    Some(PathBuf::from(std::ffi::OsString::from_vec(path)))
}

pub fn process_group(pid: u32) -> Option<u32> {
    nix::unistd::getpgid(Some(Pid::from_raw(i32::try_from(pid).ok()?)))
        .ok()
        .and_then(|group| u32::try_from(group.as_raw()).ok())
}

fn foreground_process_group(terminal: impl AsFd) -> Option<u32> {
    tcgetpgrp(terminal)
        .ok()
        .and_then(|group| u32::try_from(group.as_raw_nonzero().get()).ok())
}

#[cfg(test)]
mod tests {
    use super::{
        KodosiError, KodosiPty, PtyReader, ShutdownStage, Transfer, WaitOutcome, find_executable,
        login_shell_name, resolve_program, set_terminal_size, validate_working_dir,
    };
    use rustix::io::{FdFlags, fcntl_getfd};
    use std::{
        env,
        os::fd::{AsRawFd, BorrowedFd},
        path::Path,
        time::Duration,
    };

    async fn type_line(reader: &mut PtyReader, mut line: &[u8]) {
        let mut buffer = [0; 4096];
        while !line.is_empty() {
            match reader.transfer(&mut buffer, Some(line)).await.unwrap() {
                Transfer::Wrote(count) => line = &line[count..],
                Transfer::Read(count) => assert!(count > 0, "the shell ended before its input"),
            }
        }
    }

    async fn read_until(reader: &mut PtyReader, marker: &str) {
        let mut output = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !String::from_utf8_lossy(&output).contains(marker) {
            let mut bytes = [0; 4096];
            let count = tokio::time::timeout_at(deadline, reader.read(&mut bytes))
                .await
                .unwrap()
                .unwrap();
            assert!(count > 0, "{}", String::from_utf8_lossy(&output));
            output.extend_from_slice(&bytes[..count]);
        }
    }

    async fn end_by_force(pty: &mut KodosiPty, reader: &mut PtyReader) {
        pty.request_shutdown(ShutdownStage::Force).unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut buffer = [0; 4096];
        let mut readable = true;
        // On macOS the session leader of a terminal, and the last process that closes it, wait
        // in exit until the output of the terminal is read.
        loop {
            tokio::select! {
                outcome = pty.wait_within(deadline.saturating_duration_since(tokio::time::Instant::now())) => {
                    assert_eq!(
                        outcome.unwrap(),
                        WaitOutcome::Reaped,
                        "a process of the terminal outlived its forced shutdown"
                    );
                    return;
                }
                read = reader.read(&mut buffer), if readable => {
                    readable = matches!(read, Ok(count) if count > 0);
                }
            }
        }
    }

    #[test]
    fn other_programs_of_the_host_do_not_inherit_a_terminal() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (mut pty, mut reader) =
                    KodosiPty::spawn_program(Path::new("/bin/cat"), &[], None, 24, 80, &[], None)
                        .unwrap();
                let reader_fd = reader.pending.as_ref().unwrap().as_raw_fd();
                for descriptor in [pty.master.as_raw_fd(), reader_fd] {
                    let descriptor = unsafe { BorrowedFd::borrow_raw(descriptor) };
                    assert!(fcntl_getfd(descriptor).unwrap().contains(FdFlags::CLOEXEC));
                }
                end_by_force(&mut pty, &mut reader).await;
            });
    }

    #[test]
    fn shutdown_signals_other_processes_after_a_permission_failure() {
        let mut visited = Vec::new();
        let result = super::signal_processes(vec![10, 20, 30], |pid| {
            visited.push(pid);
            match pid {
                10 => Err(std::io::Error::from_raw_os_error(libc::EPERM)),
                20 => Err(std::io::Error::from_raw_os_error(libc::ESRCH)),
                _ => Ok(()),
            }
        });
        assert_eq!(visited, [10, 20, 30]);
        assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::EPERM));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_process_reaped_during_the_scan_does_not_count() {
        use std::io::{Error, ErrorKind};
        let live = |stat: std::io::Result<String>| super::process_is_live(stat).unwrap();
        assert!(!live(Err(Error::from_raw_os_error(libc::ESRCH))));
        assert!(!live(Err(Error::from(ErrorKind::NotFound))));
        assert!(!live(Ok("42 (sleep) Z 1 42 42".to_owned())));
        assert!(live(Ok("42 (sleep) S 1 42 42".to_owned())));
        assert!(live(Err(Error::from(ErrorKind::PermissionDenied))));
        assert!(super::process_is_live(Err(Error::from_raw_os_error(libc::EIO))).is_err());
    }

    #[test]
    fn bare_shell_name_does_not_search_the_project_before_path() {
        use std::os::unix::fs::PermissionsExt as _;
        let root = tempfile::tempdir().unwrap();
        let shadow = root.path().join("sh");
        std::fs::write(&shadow, "#!/bin/sh\nexit 42\n").unwrap();
        std::fs::set_permissions(&shadow, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_ne!(
            resolve_program(Path::new("sh"), Some(root.path())),
            Some(shadow)
        );
    }

    #[test]
    fn resizing_a_file_that_is_no_terminal_reports_the_failure() {
        let file = tempfile::tempfile().unwrap();
        let error = set_terminal_size(&file, 80, 24, None, None)
            .expect_err("TIOCSWINSZ on a file that is no terminal must fail");

        assert!(error.to_string().contains("I/O failure"));
    }

    #[test]
    fn shell_sessions_start_as_login_shells_with_a_shell_variable() {
        assert_eq!(login_shell_name(Path::new("/bin/zsh")), Some("zsh"));
        assert_eq!(login_shell_name(Path::new("/opt/bin/claude")), None);
        if !Path::new("/bin/zsh").exists() {
            return;
        }
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".zshrc"), "").unwrap();
        unsafe {
            env::set_var("ZDOTDIR", home.path());
        }
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (mut pty, mut reader) =
                    KodosiPty::spawn_program(Path::new("/bin/zsh"), &[], None, 24, 80, &[], None)
                        .unwrap();
                type_line(
                    &mut reader,
                    b"print -r -- \"probe=${options[login]}:$0:$SHELL\"; exit\n",
                )
                .await;
                let mut output = Vec::new();
                let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
                while !String::from_utf8_lossy(&output).contains("probe=on:-zsh:/bin/zsh") {
                    let mut bytes = [0; 4096];
                    let count = tokio::time::timeout_at(deadline, reader.read(&mut bytes))
                        .await
                        .unwrap()
                        .unwrap();
                    assert!(
                        count > 0,
                        "shell exited before answering: {}",
                        String::from_utf8_lossy(&output)
                    );
                    output.extend_from_slice(&bytes[..count]);
                }
                end_by_force(&mut pty, &mut reader).await;
            });
    }

    #[test]
    fn stop_waits_for_background_job_groups_after_the_shell_exits() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let root = tempfile::tempdir().unwrap();
            let (mut pty, mut reader) = KodosiPty::spawn_program(
                Path::new("/bin/sh"),
                &["-i".to_owned()], root.path().to_str(), 24, 80, &[], None
            ).unwrap();
            let command = b"sh -c 'trap \"\" HUP TERM; printf \"%s\\n\" \"$$\" > child; exec sleep 30' &\n";
            type_line(&mut reader, command).await;
            let mut buffer = [0; 4096];
            let child: i32 = tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if let Some(child) = std::fs::read_to_string(root.path().join("child")).ok().and_then(|text| text.trim().parse().ok()) {
                        break child;
                    }
                    let _ = tokio::time::timeout(Duration::from_millis(10), reader.read(&mut buffer)).await;
                }
            }).await.unwrap();
            assert_ne!(nix::unistd::getpgid(Some(nix::unistd::Pid::from_raw(child))).unwrap().as_raw(), pty.child_pid as i32);
            pty.request_shutdown(ShutdownStage::Hangup).unwrap();
            assert_eq!(pty.wait_within(Duration::from_millis(100)).await.unwrap(), WaitOutcome::TimedOut);
            assert!(super::session_processes(pty.child_pid).unwrap().contains(&child));
            end_by_force(&mut pty, &mut reader).await;
            assert_eq!(super::session_processes(pty.child_pid).unwrap(), Vec::<i32>::new());
        });
    }

    #[test]
    fn the_program_in_front_shows_its_arguments_variables_and_process_group() {
        const NAME: &str = "pty::unix::tests::the_program_in_front_shows_its_arguments_variables_and_process_group";
        if env::var_os("KODOSI_TEST_HOME").is_some() {
            std::thread::sleep(Duration::from_secs(30));
            return;
        }
        let line = format!(
            "KODOSI_TEST_HOME=/agent/home '{}' --exact {NAME} --test-threads=1\n",
            env::current_exe().unwrap().display()
        );
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (mut pty, mut reader) = KodosiPty::spawn_program(
                    Path::new("/bin/sh"),
                    &["-i".to_owned()],
                    None,
                    24,
                    80,
                    &[],
                    None,
                )
                .unwrap();
                type_line(&mut reader, line.as_bytes()).await;
                let mut buffer = [0; 4096];
                let home = tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        if let Some(home) = pty.foreground_variable("KODOSI_TEST_HOME") {
                            break home;
                        }
                        let _ = tokio::time::timeout(
                            Duration::from_millis(10),
                            reader.read(&mut buffer),
                        )
                        .await;
                    }
                })
                .await
                .unwrap();
                assert_eq!(home, "/agent/home");
                assert_eq!(pty.foreground_variable("KODOSI_TEST_MISSING"), None);
                assert_eq!(
                    pty.foreground_arguments()[1..],
                    ["--exact", NAME, "--test-threads=1"]
                );
                let group = pty.foreground_process_group().unwrap();
                assert_ne!(group, pty.child_pid);
                assert_eq!(super::process_group(group), Some(group));
                end_by_force(&mut pty, &mut reader).await;
            });
    }

    #[test]
    fn the_open_files_of_a_process_group_include_the_files_that_it_holds() {
        let folder = tempfile::tempdir().unwrap();
        let path = kodosi_os::fs::canonicalize(folder.path())
            .unwrap()
            .join("held");
        let arguments = [
            "-c",
            "exec 3>held; echo HOLDS; read line; exec 3>&-; echo RELEASED; read line",
        ]
        .map(str::to_owned);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (mut pty, mut reader) = KodosiPty::spawn_program(
                    Path::new("/bin/sh"),
                    &arguments,
                    folder.path().to_str(),
                    24,
                    80,
                    &[],
                    None,
                )
                .unwrap();
                read_until(&mut reader, "HOLDS").await;
                let group = pty.foreground_process_group().unwrap();
                assert!(super::open_files(group).unwrap().contains(&path));
                type_line(&mut reader, b"\n").await;
                read_until(&mut reader, "RELEASED").await;
                assert!(!super::open_files(group).unwrap().contains(&path));
                end_by_force(&mut pty, &mut reader).await;
            });
    }

    #[test]
    fn a_terminal_program_ends_on_signals_its_host_blocks_and_ignores() {
        use nix::sys::signal::{SigHandler, SigSet, Signal, signal};
        const NAME: &str =
            "pty::unix::tests::a_terminal_program_ends_on_signals_its_host_blocks_and_ignores";
        const HOST: &str = "KODOSI_TEST_HOST_BLOCKS_SIGNALS";
        if env::var_os(HOST).is_none() {
            let output = std::process::Command::new(env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1"])
                .env(HOST, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            return;
        }
        let signals = [Signal::SIGHUP, Signal::SIGINT, Signal::SIGTERM];
        SigSet::from_iter(signals).thread_block().unwrap();
        for host_signal in signals {
            unsafe { signal(host_signal, SigHandler::SigIgn) }.unwrap();
        }
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                for stage in [
                    ShutdownStage::Interrupt,
                    ShutdownStage::Hangup,
                    ShutdownStage::Terminate,
                ] {
                    let arguments = ["-c", "echo ready; exec sleep 30"].map(str::to_owned);
                    let (mut pty, mut reader) = KodosiPty::spawn_program(
                        Path::new("/bin/sh"),
                        &arguments,
                        None,
                        24,
                        80,
                        &[],
                        None,
                    )
                    .unwrap();
                    let mut output = Vec::new();
                    while !String::from_utf8_lossy(&output).contains("ready") {
                        let mut bytes = [0; 256];
                        let count =
                            tokio::time::timeout(Duration::from_secs(5), reader.read(&mut bytes))
                                .await
                                .unwrap()
                                .unwrap();
                        assert!(count > 0, "{stage:?}");
                        output.extend_from_slice(&bytes[..count]);
                    }
                    pty.request_shutdown(stage).unwrap();
                    assert_eq!(
                        pty.wait_within(Duration::from_secs(5)).await.unwrap(),
                        WaitOutcome::Reaped,
                        "{stage:?}"
                    );
                }
            });
    }

    #[test]
    fn a_terminal_program_gets_its_hosts_variables_and_kodosis_terminal() {
        const NAME: &str =
            "pty::unix::tests::a_terminal_program_gets_its_hosts_variables_and_kodosis_terminal";
        const HOST: &str = "KODOSI_TEST_HOST_ENVIRONMENT";
        if env::var_os(HOST).is_none() {
            let output = std::process::Command::new(env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1"])
                .env(HOST, "1")
                .env("CLAUDECODE", "1")
                .env("TERM", "xterm-ghostty")
                .env("TERMINFO", "/nowhere")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            return;
        }
        let output = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let arguments = ["-c", "env; echo END-OF-ENVIRONMENT"].map(str::to_owned);
                let (mut pty, mut reader) = KodosiPty::spawn_program(
                    Path::new("/bin/sh"),
                    &arguments,
                    None,
                    24,
                    80,
                    &[("KODOSI_SESSION_ID", "id")],
                    None,
                )
                .unwrap();
                let mut output = Vec::new();
                while !String::from_utf8_lossy(&output).contains("END-OF-ENVIRONMENT") {
                    let mut bytes = [0; 4096];
                    let count =
                        tokio::time::timeout(Duration::from_secs(5), reader.read(&mut bytes))
                            .await
                            .unwrap()
                            .unwrap();
                    assert!(count > 0, "{}", String::from_utf8_lossy(&output));
                    output.extend_from_slice(&bytes[..count]);
                }
                end_by_force(&mut pty, &mut reader).await;
                String::from_utf8_lossy_owned(output)
            });
        let variables: Vec<&str> = output.lines().map(str::trim_end).collect();
        for expected in [
            "CLAUDECODE=1",
            "TERM=xterm-256color",
            "COLORTERM=truecolor",
            "TERM_PROGRAM=ghostty",
            "TERM_PROGRAM_VERSION=1.4.1-dev",
            "KODOSI_SESSION_ID=id",
        ] {
            assert!(variables.contains(&expected), "{expected}: {output}");
        }
        assert!(
            !variables
                .iter()
                .any(|variable| variable.starts_with("TERMINFO=")),
            "{output}"
        );
    }

    #[test]
    fn invalid_working_directory_is_rejected_before_launch() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let missing = directory.path().join("missing");
        let error = validate_working_dir(missing.to_str())
            .expect_err("missing working directory must fail closed");
        assert!(
            error
                .to_string()
                .contains("working directory does not exist")
        );

        let file = directory.path().join("file");
        std::fs::write(&file, "not a directory").expect("write file");
        assert!(validate_working_dir(file.to_str()).is_err());
    }

    #[test]
    fn a_program_that_cannot_start_fails_its_spawn() {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("program");
        kodosi_os::process::write_program(&program, "#!/nonexistent/interpreter\n").unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _context = runtime.enter();
        let error = KodosiPty::spawn_program(&program, &[], None, 24, 80, &[], None).unwrap_err();
        assert!(
            matches!(&error, KodosiError::Io(error) if error.kind() == std::io::ErrorKind::NotFound),
            "{error}"
        );
    }

    #[test]
    fn configured_shell_resolution_never_substitutes_another_shell() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let missing = directory.path().join("missing-shell");
        assert_eq!(resolve_program(&missing, None), None);
    }

    #[test]
    fn executable_resolution_requires_an_execute_bit() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("candidate");
        std::fs::write(&path, "#!/bin/sh\n").expect("write candidate");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("set non-executable permissions");
        assert_eq!(find_executable(&path), None);

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
            .expect("set executable permissions");
        assert_eq!(find_executable(&path), Some(path));
    }
}
