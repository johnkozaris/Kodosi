use std::{
    env,
    fs::File,
    io,
    os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd},
    path::{Path, PathBuf},
    time::Duration,
};

use kodosi_utils::{KodosiError, Result};
use nix::{
    fcntl::{FcntlArg, OFlag, fcntl},
    pty::{Winsize, openpty},
    sys::{
        signal::{Signal, killpg},
        termios,
    },
    unistd::{self, Pid},
};
use tokio::{
    io::unix::AsyncFd,
    process::{Child, Command},
};

use super::{ShutdownStage, WaitOutcome};

#[derive(Debug)]
pub struct RawFdAsyncReader {
    pending: Option<File>,
    async_fd: Option<AsyncFd<File>>,
}

impl RawFdAsyncReader {
    pub fn new(fd: OwnedFd) -> io::Result<Self> {
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

    pub fn try_read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let async_fd = self
            .async_fd
            .as_ref()
            .ok_or_else(|| io::Error::other("PTY reader not yet initialized"))?;
        match unistd::read(async_fd.get_ref(), buf) {
            Ok(n) => Ok(n),
            Err(nix::errno::Errno::EAGAIN) => Err(io::Error::from(io::ErrorKind::WouldBlock)),
            Err(error) => Err(io::Error::from_raw_os_error(error as i32)),
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
    pub fn spawn_shell(
        initial_shell: Option<&str>,
        working_dir: Option<&str>,
        rows: u16,
        cols: u16,
        pane_identity: &str,
    ) -> Result<(Self, RawFdAsyncReader)> {
        Self::spawn_shell_with_env(initial_shell, working_dir, rows, cols, pane_identity, &[])
    }

    pub fn spawn_shell_with_env(
        initial_shell: Option<&str>,
        working_dir: Option<&str>,
        rows: u16,
        cols: u16,
        pane_identity: &str,
        session_environment: &[(String, String)],
    ) -> Result<(Self, RawFdAsyncReader)> {
        let program = shell_program(initial_shell);
        Self::spawn_program_with_env(
            &program,
            &[],
            working_dir,
            rows,
            cols,
            pane_identity,
            session_environment,
        )
    }

    pub fn spawn_program_with_env(
        program: &Path,
        arguments: &[String],
        working_dir: Option<&str>,
        rows: u16,
        cols: u16,
        pane_identity: &str,
        session_environment: &[(String, String)],
    ) -> Result<(Self, RawFdAsyncReader)> {
        let working_dir = validate_working_dir(working_dir)?;
        let resolved_program = resolve_program(program, working_dir).ok_or_else(|| {
            KodosiError::Spawn(format!("command not found: {}", program.display()))
        })?;

        let current_termios = termios::tcgetattr(io::stdin()).ok();
        if current_termios.is_none() {
            tracing::warn!(
                "starting Kodosi PTY without a controlling terminal; falling back to default termios"
            );
        }
        let openpty_result = openpty(None, &current_termios)
            .map_err(|error| io::Error::from_raw_os_error(error as i32))?;
        let master_raw = openpty_result.master.as_raw_fd();
        let slave_raw = openpty_result.slave.as_raw_fd();
        set_terminal_size_using_fd(master_raw, cols, rows, None, None)?;

        let reader_fd = unistd::dup(&openpty_result.master)
            .map_err(|error| io::Error::from_raw_os_error(error as i32))?;
        let reader = RawFdAsyncReader::new(reader_fd)?;

        let mut command = Command::new(&resolved_program);
        command.args(arguments);
        if let Some(directory) = working_dir {
            command.current_dir(directory);
        }
        command.env("KODOSI_SESSION_ID", pane_identity);
        command.envs(session_environment.iter().map(|(key, value)| (key, value)));
        if env::var_os("TERM").is_none() {
            command.env("TERM", "xterm-256color");
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

    pub fn write(&mut self, buf: &[u8]) -> Result<usize> {
        try_write_to_fd(self.master_fd, buf).map_err(|error| {
            KodosiError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!("failed to write to PTY stdin: {error}"),
            ))
        })
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
        self.send_signal(signal)?;
        if stage == ShutdownStage::Force
            && foreground_process_group(self.master_fd).is_some_and(|group| group != self.child_pid)
        {
            match self.send_shell_group_signal(signal) {
                Ok(()) => {}
                Err(error) if is_missing_process_group(&error) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    pub fn tcdrain(&self) -> Result<()> {
        let fd = unsafe { BorrowedFd::borrow_raw(self.master_fd) };
        termios::tcdrain(fd).map_err(|error| {
            KodosiError::Io(io::Error::other(format!(
                "failed to drain PTY output for child {}: {error}",
                self.child_pid
            )))
        })
    }

    pub async fn wait(&mut self) -> Result<Option<i32>> {
        let status = self.child.wait().await?;
        Ok(status.code())
    }

    pub async fn wait_within(&mut self, grace: Duration) -> Result<WaitOutcome> {
        match tokio::time::timeout(grace, self.wait()).await {
            Ok(result) => result.map(WaitOutcome::Reaped),
            Err(_) => Ok(WaitOutcome::TimedOut),
        }
    }

    pub fn child_pid(&self) -> u32 {
        self.child_pid
    }

    pub fn foreground_process_group_id(&self) -> Option<u32> {
        foreground_process_group(self.master_fd)
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
                signal_process_group(shell_group, signal)
            }
            result => result,
        }
    }

    fn send_shell_group_signal(&self, signal: Signal) -> Result<()> {
        let shell_group = Pid::from_raw(self.child_pid as i32);
        if shell_group == unistd::getpgrp() || shell_group == unistd::getpid() {
            return Err(KodosiError::Io(io::Error::other(
                "refusing to signal Kodosi's own process or process group",
            )));
        }
        if !process_group_belongs_to_session(shell_group, shell_group) {
            return Ok(());
        }
        signal_process_group(shell_group, signal)
    }
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
        let foreground = foreground_process_group(self.master_fd);
        let _ = self.send_signal(Signal::SIGKILL);
        if foreground.is_some_and(|group| group != self.child_pid) {
            let _ = self.send_shell_group_signal(Signal::SIGKILL);
        }

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

fn try_write_to_fd(fd: RawFd, buf: &[u8]) -> std::result::Result<usize, nix::Error> {
    let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
    let mut written = 0;
    while written < buf.len() {
        match unistd::write(borrowed, &buf[written..]) {
            Ok(0) => break,
            Ok(count) => written += count,
            Err(nix::errno::Errno::EINTR) => continue,
            Err(nix::errno::Errno::EAGAIN) => break,
            Err(error) => return Err(error),
        }
    }
    Ok(written)
}

fn shell_program(initial_shell: Option<&str>) -> PathBuf {
    initial_shell
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(get_default_shell)
}

fn validate_working_dir(working_dir: Option<&str>) -> Result<Option<&Path>> {
    let Some(directory) = working_dir.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let directory = Path::new(directory);
    if directory.is_dir() {
        Ok(Some(directory))
    } else {
        Err(KodosiError::Spawn(format!(
            "working directory does not exist or is not a directory: {}",
            directory.display()
        )))
    }
}

fn get_default_shell() -> PathBuf {
    PathBuf::from(std::env::var("SHELL").unwrap_or_else(|_| {
        tracing::warn!("Cannot read SHELL env, falling back to use /bin/sh");
        "/bin/sh".to_owned()
    }))
}

fn find_executable(candidate: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;

    let metadata = candidate.metadata().ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .then(|| candidate.to_path_buf())
}

fn resolve_program(program: &Path, working_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(cwd) = working_dir
        && let Some(resolved) = find_executable(&cwd.join(program))
    {
        return Some(resolved);
    }
    if let Some(resolved) = find_executable(program) {
        return Some(resolved);
    }
    let paths = env::var_os("PATH")?;
    for path in env::split_paths(&paths) {
        if let Some(resolved) = find_executable(&path.join(program)) {
            return Some(resolved);
        }
    }
    None
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
        ShutdownStage, Signal, WaitOutcome, find_executable, openpty, resolve_program,
        set_terminal_size_using_fd, signal_for_shutdown_stage, try_write_to_fd,
        validate_working_dir,
    };
    use nix::{
        fcntl::{FcntlArg, OFlag, fcntl},
        sys::termios,
    };
    use std::{io::Read, os::fd::AsRawFd};

    #[test]
    fn resizing_an_invalid_pty_descriptor_reports_the_ioctl_failure() {
        let error = set_terminal_size_using_fd(-1, 80, 24, None, None)
            .expect_err("TIOCSWINSZ on an invalid descriptor must fail");

        assert!(error.to_string().contains("I/O failure"));
    }

    #[test]
    fn try_write_to_fd_returns_partial_on_full_buffer() {
        let pty = openpty(None, &None).expect("openpty failed");

        let mut attrs = termios::tcgetattr(&pty.slave).expect("tcgetattr failed");
        termios::cfmakeraw(&mut attrs);
        termios::tcsetattr(&pty.slave, termios::SetArg::TCSANOW, &attrs).expect("tcsetattr failed");

        let flags = fcntl(&pty.master, FcntlArg::F_GETFL).expect("F_GETFL");
        let mut oflags = OFlag::from_bits_truncate(flags);
        oflags.insert(OFlag::O_NONBLOCK);
        fcntl(&pty.master, FcntlArg::F_SETFL(oflags)).expect("F_SETFL");

        let master_raw = pty.master.as_raw_fd();
        let chunk = vec![0x42u8; 1024];
        let mut total_filled = 0;
        loop {
            match try_write_to_fd(master_raw, &chunk) {
                Ok(0) => break,
                Ok(n) => total_filled += n,
                Err(error) => panic!("unexpected error filling buffer: {error}"),
            }
        }
        assert!(
            total_filled > 0,
            "should have written some bytes to fill buffer"
        );

        let slave_file = std::fs::File::from(pty.slave);
        let mut slave_reader = std::io::BufReader::new(&slave_file);
        let mut drain = vec![0u8; 512];
        let drained = slave_reader.read(&mut drain).expect("slave read failed");
        assert!(drained > 0, "should have drained some bytes");

        let size = 128 * 1024;
        let data: Vec<u8> = (0..size)
            .map(|index| u8::try_from(index % 256).unwrap())
            .collect();
        let written =
            try_write_to_fd(master_raw, &data).expect("try_write_to_fd should not error on EAGAIN");

        assert!(
            written > 0 && written < size,
            "expected partial write, got {written}/{size}"
        );
    }

    #[test]
    fn try_write_to_fd_returns_zero_on_stuck_pty() {
        let pty = openpty(None, &None).expect("openpty failed");

        let mut attrs = termios::tcgetattr(&pty.slave).expect("tcgetattr failed");
        termios::cfmakeraw(&mut attrs);
        termios::tcsetattr(&pty.slave, termios::SetArg::TCSANOW, &attrs).expect("tcsetattr failed");

        let flags = fcntl(&pty.master, FcntlArg::F_GETFL).expect("F_GETFL");
        let mut oflags = OFlag::from_bits_truncate(flags);
        oflags.insert(OFlag::O_NONBLOCK);
        fcntl(&pty.master, FcntlArg::F_SETFL(oflags)).expect("F_SETFL");

        let master_raw = pty.master.as_raw_fd();
        let fill = vec![0x42u8; 1024];
        loop {
            match try_write_to_fd(master_raw, &fill) {
                Ok(0) => break,
                Ok(_) => continue,
                Err(error) => panic!("unexpected error filling buffer: {error}"),
            }
        }

        let written = try_write_to_fd(master_raw, &[0x01, 0x02, 0x03])
            .expect("try_write_to_fd should not error on EAGAIN");
        assert_eq!(written, 0, "expected zero bytes written on full buffer");
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

    #[test]
    fn wait_outcomes_do_not_conflate_signal_exit_with_timeout() {
        assert_ne!(WaitOutcome::Reaped(None), WaitOutcome::TimedOut);
        assert_ne!(WaitOutcome::Reaped(Some(0)), WaitOutcome::TimedOut);
    }

    #[test]
    fn shutdown_stages_map_to_unix_process_group_signals() {
        assert_eq!(
            signal_for_shutdown_stage(ShutdownStage::Interrupt),
            Signal::SIGINT
        );
        assert_eq!(
            signal_for_shutdown_stage(ShutdownStage::Hangup),
            Signal::SIGHUP
        );
        assert_eq!(
            signal_for_shutdown_stage(ShutdownStage::Terminate),
            Signal::SIGTERM
        );
        assert_eq!(
            signal_for_shutdown_stage(ShutdownStage::Force),
            Signal::SIGKILL
        );
    }
}
