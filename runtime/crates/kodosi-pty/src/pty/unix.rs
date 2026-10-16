use std::{
    env,
    fs::File,
    io,
    os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd},
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::Duration,
};

use crate::{KodosiError, Result};
use nix::{
    fcntl::{FcntlArg, OFlag, fcntl},
    pty::{Winsize, openpty},
    sys::signal::{Signal, killpg},
    unistd::{self, Pid},
};
use tokio::{
    io::{Interest, Ready, unix::AsyncFd},
    process::{Child, Command},
};

use super::{
    ShutdownStage, Transfer, WaitOutcome, inherits_foreign_context, program_identity,
    validate_working_dir,
};

static OPEN_PTY: Mutex<()> = Mutex::new(());

#[derive(Debug)]
pub struct PtyReader {
    pending: Option<File>,
    async_fd: Option<AsyncFd<File>>,
}

impl PtyReader {
    fn new(fd: OwnedFd) -> io::Result<Self> {
        let flags = fcntl(fd.as_fd(), FcntlArg::F_GETFL)
            .map_err(|error| io::Error::from_raw_os_error(error as i32))?;
        let mut oflags = OFlag::from_bits_truncate(flags);
        oflags.insert(OFlag::O_NONBLOCK);
        fcntl(fd.as_fd(), FcntlArg::F_SETFL(oflags))
            .map_err(|error| io::Error::from_raw_os_error(error as i32))?;

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
            self.async_fd = Some(AsyncFd::new(file)?);
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
            match guard.try_io(|inner| {
                unistd::read(inner.get_ref(), buf)
                    .map_err(|error| io::Error::from_raw_os_error(error as i32))
            }) {
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
                match unistd::write(guard.get_inner(), bytes) {
                    Ok(0) | Err(nix::errno::Errno::EAGAIN) => {
                        guard.clear_ready_matching(Ready::WRITABLE);
                    }
                    Ok(count) => return Ok(Transfer::Wrote(count)),
                    Err(nix::errno::Errno::EINTR) => {}
                    Err(error) => return Err(io::Error::from_raw_os_error(error as i32)),
                }
            }
            if ready.is_readable() {
                match unistd::read(guard.get_inner(), read) {
                    Ok(count) => return Ok(Transfer::Read(count)),
                    Err(nix::errno::Errno::EAGAIN) => guard.clear_ready_matching(Ready::READABLE),
                    Err(nix::errno::Errno::EINTR) => {}
                    Err(error) => return Err(io::Error::from_raw_os_error(error as i32)),
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct KodosiPty {
    master_fd: RawFd,
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
    ) -> Result<(Self, PtyReader)> {
        let working_dir = validate_working_dir(working_dir)?;
        let resolved_program = resolve_program(program, working_dir).ok_or_else(|| {
            KodosiError::Spawn(format!("command not found: {}", program.display()))
        })?;

        let openpty_result = {
            let _serial = OPEN_PTY.lock().unwrap_or_else(PoisonError::into_inner);
            openpty(None, &None).map_err(|error| io::Error::from_raw_os_error(error as i32))?
        };
        let master_raw = openpty_result.master.as_raw_fd();
        let slave_raw = openpty_result.slave.as_raw_fd();
        set_terminal_size_using_fd(master_raw, cols, rows, None, None)?;

        let reader_fd = unistd::dup(&openpty_result.master)
            .map_err(|error| io::Error::from_raw_os_error(error as i32))?;
        let reader = PtyReader::new(reader_fd)?;

        let mut command = Command::new(&resolved_program);
        command.args(arguments);
        if arguments.is_empty()
            && let Some(shell) = login_shell_name(&resolved_program)
        {
            command.arg0(format!("-{shell}"));
            command.env("SHELL", &resolved_program);
        }
        if let Some(directory) = working_dir {
            command.current_dir(directory);
        }
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("TERM_PROGRAM", "Kodosi");
        if env::var_os("LANG").is_none() && env::var_os("LC_ALL").is_none() {
            command.env("LANG", "en_US.UTF-8");
        }
        for (key, _) in env::vars_os() {
            if key.to_str().is_some_and(inherits_foreign_context) {
                command.env_remove(&key);
            }
        }

        for (key, value) in environment {
            command.env(key, value);
        }
        command.kill_on_drop(true);

        unsafe {
            command.pre_exec(move || {
                if libc::login_tty(slave_raw) != 0 {
                    return Err(io::Error::last_os_error());
                }
                close_fds::close_open_fds(3, &[]);
                Ok(())
            });
        }

        let child = command.spawn()?;
        let child_pid = child.id().ok_or_else(|| {
            KodosiError::Spawn("spawned shell did not report a process id".to_owned())
        })?;

        drop(openpty_result.slave);

        Ok((
            Self {
                master_fd: openpty_result.master.into_raw_fd(),
                child_pid,
                child,
            },
            reader,
        ))
    }

    pub fn working_directory(&self) -> Option<PathBuf> {
        let pid = foreground_process_group(self.master_fd).unwrap_or(self.child_pid);
        process_directory(pid).or_else(|| process_directory(self.child_pid))
    }

    pub fn foreground_process_group(&self) -> Option<u32> {
        foreground_process_group(self.master_fd)
    }

    pub fn foreground_is_child(&self) -> bool {
        foreground_process_group(self.master_fd) == Some(self.child_pid)
    }

    pub fn reads_keys(&self) -> bool {
        let borrowed = unsafe { BorrowedFd::borrow_raw(self.master_fd) };
        nix::sys::termios::tcgetattr(borrowed).is_ok_and(|attributes| {
            !attributes
                .local_flags
                .contains(nix::sys::termios::LocalFlags::ICANON)
        })
    }

    pub fn foreground_program(&self) -> Option<String> {
        let pid = foreground_process_group(self.master_fd)?;
        let arguments = process_arguments(pid)?;
        program_identity(&arguments).map(str::to_owned)
    }

    pub fn resize(
        &self,
        rows: u16,
        cols: u16,
        width_pixels: Option<u16>,
        height_pixels: Option<u16>,
    ) -> Result<()> {
        set_terminal_size_using_fd(self.master_fd, cols, rows, width_pixels, height_pixels)
    }

    pub fn request_shutdown(&mut self, stage: ShutdownStage) -> Result<()> {
        let signal = signal_for_shutdown_stage(stage);
        if stage == ShutdownStage::Interrupt {
            return self.send_signal(signal);
        }
        signal_processes(session_processes(self.child_pid)?, |process| {
            if process_belongs_to_session(process, self.child_pid) {
                nix::sys::signal::kill(Pid::from_raw(process), signal)
                    .map_err(|error| io::Error::from_raw_os_error(error as i32))
            } else {
                Ok(())
            }
        })?;
        Ok(())
    }

    async fn wait(&mut self) -> Result<()> {
        self.child.wait().await?;
        Ok(())
    }

    pub async fn wait_within(&mut self, grace: Duration) -> Result<WaitOutcome> {
        match tokio::time::timeout(grace, async {
            self.wait().await?;
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
        foreground_process_group(self.master_fd)
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
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        let size = i32::try_from(std::mem::size_of_val(&info))
            .map_err(|_| io::Error::other("invalid native process info size"))?;
        let read = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        if read != size {
            if process_belongs_to_session(pid, session) {
                processes.push(pid);
            }
            continue;
        }
        if unsafe { info.assume_init() }.pbi_status != libc::SZOMB {
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
        Err(error) => Err(KodosiError::Io(io::Error::from_raw_os_error(error as i32))),
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
        let _ = unsafe { OwnedFd::from_raw_fd(self.master_fd) };
    }
}

fn set_terminal_size_using_fd(
    fd: RawFd,
    columns: u16,
    rows: u16,
    width_in_pixels: Option<u16>,
    height_in_pixels: Option<u16>,
) -> Result<()> {
    let winsize = Winsize {
        ws_col: columns,
        ws_row: rows,
        ws_xpixel: width_in_pixels.unwrap_or(0),
        ws_ypixel: height_in_pixels.unwrap_or(0),
    };

    let result = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &winsize) };
    if result == -1 {
        Err(KodosiError::Io(io::Error::last_os_error()))
    } else {
        Ok(())
    }
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
    use std::os::unix::ffi::OsStringExt as _;
    let mut info = std::mem::MaybeUninit::<libc::proc_vnodepathinfo>::zeroed();
    let size = i32::try_from(std::mem::size_of_val(&info)).ok()?;
    let result = unsafe {
        libc::proc_pidinfo(
            i32::try_from(pid).ok()?,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if result != size {
        return None;
    }
    let info = unsafe { info.assume_init() };
    let bytes = info
        .pvi_cdir
        .vip_path
        .iter()
        .flatten()
        .take_while(|byte| **byte != 0)
        .map(|byte| byte.to_ne_bytes()[0])
        .collect::<Vec<_>>();
    if bytes.is_empty() {
        return None;
    }
    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

#[cfg(target_os = "linux")]
fn process_arguments(pid: u32) -> Option<Vec<String>> {
    Some(strings(
        &std::fs::read(format!("/proc/{pid}/cmdline")).ok()?,
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
    Some(arguments)
}

fn foreground_process_group(fd: RawFd) -> Option<u32> {
    let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
    nix::unistd::tcgetpgrp(borrowed)
        .ok()
        .map(Pid::as_raw)
        .filter(|pgid| *pgid > 0)
        .and_then(|pgid| u32::try_from(pgid).ok())
}

#[cfg(test)]
mod tests {
    use super::{
        KodosiPty, PtyReader, ShutdownStage, Transfer, find_executable, login_shell_name,
        resolve_program, set_terminal_size_using_fd, validate_working_dir,
    };
    use std::{env, path::Path, time::Duration};

    async fn type_line(reader: &mut PtyReader, mut line: &[u8]) {
        let mut buffer = [0; 4096];
        while !line.is_empty() {
            match reader.transfer(&mut buffer, Some(line)).await.unwrap() {
                Transfer::Wrote(count) => line = &line[count..],
                Transfer::Read(count) => assert!(count > 0, "the shell ended before its input"),
            }
        }
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
    fn resizing_an_invalid_pty_descriptor_reports_the_ioctl_failure() {
        let error = set_terminal_size_using_fd(-1, 80, 24, None, None)
            .expect_err("TIOCSWINSZ on an invalid descriptor must fail");

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
                    KodosiPty::spawn_program(Path::new("/bin/zsh"), &[], None, 24, 80, &[])
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
                pty.request_shutdown(ShutdownStage::Force).unwrap();
                pty.wait().await.unwrap();
            });
    }

    #[test]
    fn stop_waits_for_background_job_groups_after_the_shell_exits() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let root = tempfile::tempdir().unwrap();
            let (mut pty, mut reader) = KodosiPty::spawn_program(
                Path::new("/bin/sh"),
                &["-i".to_owned()], root.path().to_str(), 24, 80, &[]
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
            assert_eq!(pty.wait_within(Duration::from_millis(100)).await.unwrap(), super::WaitOutcome::TimedOut);
            assert!(super::session_processes(pty.child_pid).unwrap().contains(&child));
            pty.request_shutdown(ShutdownStage::Force).unwrap();
            assert!(matches!(pty.wait_within(Duration::from_secs(2)).await.unwrap(), super::WaitOutcome::Reaped));
            assert_eq!(super::session_processes(pty.child_pid).unwrap(), Vec::<i32>::new());
        });
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
