use std::{
    collections::VecDeque,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
};

use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader},
    sync::{mpsc, oneshot},
};

use super::engine::Message;

const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
const STDERR_TAIL_BYTES: usize = 4 * 1024;
const INPUT_LINES: usize = 32;
#[cfg(unix)]
const END_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) struct Spawn<'a> {
    pub program: &'a Path,
    pub arguments: &'a [String],
    pub directory: &'a Path,
    pub path: Option<OsString>,
    pub variables: Vec<(String, OsString)>,
    pub input: Option<String>,
}

pub(super) struct Child {
    pub id: u64,
    pid: Option<u32>,
    input: Option<mpsc::Sender<String>>,
    end: Option<oneshot::Sender<()>>,
}

impl Child {
    pub(super) fn write(&self, line: String) -> bool {
        self.input
            .as_ref()
            .is_some_and(|input| input.try_send(line).is_ok())
    }

    pub(super) fn end(&mut self) {
        if let Some(end) = self.end.take() {
            let _ = end.send(());
        }
    }

    pub(super) fn end_now(mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            kodosi_os::process::end_group(pid, true);
        }
        #[cfg(not(unix))]
        let _ = self.pid;
        self.end();
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        self.end();
    }
}

pub(super) fn spawn(
    id: u64,
    spawn: Spawn<'_>,
    messages: mpsc::Sender<Message>,
) -> std::io::Result<Child> {
    let mut command = tokio::process::Command::new(spawn.program);
    command
        .args(spawn.arguments)
        .current_dir(spawn.directory)
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(path) = &spawn.path {
        command.env("PATH", path);
    }
    for (name, value) in &spawn.variables {
        command.env(name, value);
    }
    kodosi_os::process::without_terminal(command.as_std_mut());
    let mut child = command.spawn()?;
    let pid = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut stdin = child.stdin.take();
    let input = if let Some(text) = spawn.input {
        if let Some(mut writer) = stdin.take() {
            tokio::spawn(async move {
                drop(writer.write_all(text.as_bytes()).await);
                drop(writer.shutdown().await);
            });
        }
        None
    } else {
        stdin.map(|mut writer| {
            let (sender, mut lines) = mpsc::channel::<String>(INPUT_LINES);
            tokio::spawn(async move {
                while let Some(mut line) = lines.recv().await {
                    line.push('\n');
                    if writer.write_all(line.as_bytes()).await.is_err()
                        || writer.flush().await.is_err()
                    {
                        break;
                    }
                }
            });
            sender
        })
    };
    let (end, ended) = oneshot::channel();
    let tail = tokio::spawn(stderr_tail(stderr));
    let reader = {
        let messages = messages.clone();
        tokio::spawn(async move {
            if let Some(stdout) = stdout {
                read_lines(BufReader::new(stdout), |line| {
                    let messages = messages.clone();
                    async move {
                        messages
                            .send(Message::Line { process: id, line })
                            .await
                            .is_ok()
                    }
                })
                .await;
            }
        })
    };
    tokio::spawn(async move {
        let status = tokio::select! {
            status = child.wait() => {
                #[cfg(unix)]
                if let Some(pid) = pid {
                    kodosi_os::process::end_group(pid, true);
                }
                status.ok()
            }
            _ = ended => {
                stop(pid, &mut child).await;
                None
            }
        };
        drop(reader.await);
        let tail = tail.await.unwrap_or_default();
        let failed = status.is_none_or(|status| !status.success());
        drop(
            messages
                .send(Message::Exited {
                    process: id,
                    error: failed
                        .then_some(tail)
                        .filter(|tail| !tail.trim().is_empty()),
                })
                .await,
        );
    });
    Ok(Child {
        id,
        pid,
        input,
        end: Some(end),
    })
}

async fn stop(pid: Option<u32>, child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = pid {
        kodosi_os::process::end_group(pid, false);
        let exited = tokio::time::timeout(END_GRACE, child.wait()).await.is_ok();
        kodosi_os::process::end_group(pid, true);
        if exited {
            return;
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
    drop(child.start_kill());
    drop(child.wait().await);
}

async fn stderr_tail(stderr: Option<tokio::process::ChildStderr>) -> String {
    let Some(mut stderr) = stderr else {
        return String::new();
    };
    let mut tail = VecDeque::with_capacity(STDERR_TAIL_BYTES);
    let mut chunk = [0_u8; 4096];
    while let Ok(count) = stderr.read(&mut chunk).await {
        if count == 0 {
            break;
        }
        tail.extend(&chunk[..count]);
        while tail.len() > STDERR_TAIL_BYTES {
            tail.pop_front();
        }
    }
    String::from_utf8_lossy(&tail.into_iter().collect::<Vec<_>>()).into_owned()
}

pub(super) async fn read_lines<R, F, Fut>(mut reader: R, mut deliver: F)
where
    R: AsyncBufRead + Unpin,
    F: FnMut(String) -> Fut,
    Fut: Future<Output = bool>,
{
    let mut line = Vec::new();
    let mut skipping = false;
    loop {
        let Ok(available) = reader.fill_buf().await else {
            return;
        };
        if available.is_empty() {
            return;
        }
        let complete = available
            .iter()
            .position(|&byte| byte == b'\n')
            .map(|end| end + 1);
        let part = complete.map_or(available, |end| &available[..end - 1]);
        if !skipping {
            if line.len() + part.len() > MAX_LINE_BYTES {
                line.clear();
                skipping = true;
            } else {
                line.extend_from_slice(part);
            }
        }
        let used = complete.unwrap_or(available.len());
        reader.consume(used);
        if complete.is_some() {
            let finished = std::mem::take(&mut line);
            if !std::mem::replace(&mut skipping, false)
                && !deliver(String::from_utf8_lossy(&finished).into_owned()).await
            {
                return;
            }
        }
    }
}

pub(super) fn executable_folder() -> Option<PathBuf> {
    let folder = std::env::current_exe().ok()?.parent()?.to_path_buf();
    folder
        .join(if cfg!(windows) {
            "kodosi.exe"
        } else {
            "kodosi"
        })
        .is_file()
        .then_some(folder)
}
