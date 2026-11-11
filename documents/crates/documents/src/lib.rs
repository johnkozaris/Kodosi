//! Runs open documents. One task owns each document and does its disk and timer work, so a
//! document needs no locks. A host gives a path and a viewer, and it gets a [`Pipe`].

mod journal;
mod live;
mod task;

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

pub use document::Viewer;
use document::{Input, PipeId};
pub use live::{Formats, Live, Opener, format};
use sha2::{Digest as _, Sha256};
use task::Command;
use tokio::sync::mpsc;

const MOST_OPEN: usize = 8;
const MOST_FILE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("{0} documents are open on this computer. Close one first.")]
    TooMany(usize),
    #[error("This file is larger than {} MB.", MOST_FILE_BYTES / 1024 / 1024)]
    TooLarge,
    #[error("This kind of file cannot open here.")]
    UnknownKind,
    #[error(transparent)]
    Unreadable(#[from] document::Unreadable),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("The document closed.")]
pub struct Closed;

struct Open {
    commands: mpsc::Sender<Command>,
}

struct Shared {
    state: PathBuf,
    formats: Formats,
    open: Mutex<HashMap<PathBuf, Open>>,
    /// One document starts at a time, so viewers of one file never start it twice.
    starting: tokio::sync::Mutex<()>,
    next_pipe: AtomicU64,
}

/// The open documents of one computer.
#[derive(Clone)]
pub struct Documents {
    shared: Arc<Shared>,
}

impl Documents {
    pub fn new(state_folder: impl Into<PathBuf>, formats: Formats) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: state_folder.into(),
                formats,
                open: Mutex::default(),
                starting: tokio::sync::Mutex::default(),
                next_pipe: AtomicU64::new(1),
            }),
        }
    }

    /// Opens a file, or joins it if it is open, and gives a viewer its pipe. The viewer's first
    /// frame is `join`.
    pub async fn open(&self, path: &Path, viewer: Viewer) -> Result<Pipe, OpenError> {
        let path = real_path(path)?;
        let commands = self.task_of(&path).await?;
        let pipe = self.shared.next_pipe.fetch_add(1, Ordering::Relaxed);
        let (outgoing, incoming) = mpsc::unbounded_channel();
        let waiting = Arc::new(AtomicUsize::new(0));
        let connected = Command::Connect {
            pipe,
            viewer,
            outgoing,
            waiting: Arc::clone(&waiting),
        };
        if commands.send(connected).await.is_err() {
            return Err(io::Error::other("the document closed").into());
        }
        Ok(Pipe {
            id: pipe,
            commands,
            incoming,
            waiting,
        })
    }

    async fn task_of(&self, path: &Path) -> Result<mpsc::Sender<Command>, OpenError> {
        if let Some(commands) = self.running(path) {
            return Ok(commands);
        }
        let _starting = self.shared.starting.lock().await;
        if let Some(commands) = self.running(path) {
            return Ok(commands);
        }
        if self.lock().len() >= MOST_OPEN {
            return Err(OpenError::TooMany(MOST_OPEN));
        }
        let format = self.shared.formats.of(path).ok_or(OpenError::UnknownKind)?;
        let size = tokio::fs::metadata(path).await?.len();
        if size > MOST_FILE_BYTES {
            return Err(OpenError::TooLarge);
        }
        let folder = self.shared.state.join(name_of(path));
        let commands = task::start(path.to_owned(), folder, format).await?;
        let open = Open {
            commands: commands.clone(),
        };
        self.lock().insert(path.to_owned(), open);
        Ok(commands)
    }

    /// The task of an open file, after the tasks that ended go.
    fn running(&self, path: &Path) -> Option<mpsc::Sender<Command>> {
        let mut open = self.lock();
        open.retain(|_, document| !document.commands.is_closed());
        open.get(path).map(|document| document.commands.clone())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Open>> {
        self.shared
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Saves and closes each document, and leaves no journal behind.
    pub async fn close(&self) {
        let open: Vec<_> = self.lock().drain().map(|(_, open)| open.commands).collect();
        for commands in open {
            let (done, finished) = tokio::sync::oneshot::channel();
            if commands.send(Command::Close { done }).await.is_ok() {
                drop(finished.await);
            }
        }
    }
}

/// The one path of a file, so that each spelling of it, such as one through `..` or a link, opens
/// the same document.
pub fn real_path(path: &Path) -> io::Result<PathBuf> {
    let real = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    if let Some(plain) = real
        .to_str()
        .and_then(|text| text.strip_prefix(r"\\?\"))
        .filter(|plain| plain.get(1..3) == Some(r":\"))
    {
        return Ok(PathBuf::from(plain));
    }
    Ok(real)
}

fn name_of(path: &Path) -> String {
    let digest = Sha256::digest(path.as_os_str().as_encoded_bytes());
    digest
        .iter()
        .take(16)
        .fold(String::new(), |text, byte| text + &format!("{byte:02x}"))
}

/// A viewer's end of a document. The host carries these frames in any way it likes.
pub struct Pipe {
    id: PipeId,
    commands: mpsc::Sender<Command>,
    incoming: mpsc::UnboundedReceiver<Arc<[u8]>>,
    waiting: Arc<AtomicUsize>,
}

impl Pipe {
    pub async fn send(&self, frame: Vec<u8>) -> Result<(), Closed> {
        let input = Input::Message {
            pipe: self.id,
            frame,
        };
        self.commands
            .send(Command::Input(input))
            .await
            .map_err(|_| Closed)
    }

    /// The next frame for the view, or nothing when the document ended this pipe.
    pub async fn receive(&mut self) -> Option<Arc<[u8]>> {
        let frame = self.incoming.recv().await?;
        self.waiting.fetch_sub(frame.len(), Ordering::Relaxed);
        Some(frame)
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        let left = Command::Input(Input::Left { pipe: self.id });
        if let Err(mpsc::error::TrySendError::Full(left)) = self.commands.try_send(left) {
            let commands = self.commands.clone();
            tokio::spawn(async move {
                drop(commands.send(left).await);
            });
        }
    }
}
