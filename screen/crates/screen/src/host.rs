//! The host side: starts the helper when the first viewer comes and gives each viewer a pipe.

use std::{
    ffi::OsString,
    fmt,
    fmt::Write as _,
    io,
    path::PathBuf,
    sync::{
        Arc, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use stream::viewer::{self, Viewer};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    process::Command,
    sync::{Mutex, mpsc, oneshot},
};

use crate::{
    link::{self, Access, Away, FromHelper, Message, Notice, VERSION},
    live::{Calls, Event, Live, Mailbox, Wanted},
    wire::ToScreen,
};

const EVENTS: usize = 1024;
const VIEWER_QUEUE: usize = 1024;
#[cfg(target_os = "macos")]
const CONNECT_LIMIT: Duration = Duration::from_secs(30);
/// A helper that runs as the host's child says hello once it reached the desktop, which takes
/// a while on a busy computer.
const HELLO_LIMIT: Duration = Duration::from_secs(15);
/// Join number, so that a closing pipe only removes its own seat and not a reconnected one.
static SEATS: AtomicU64 = AtomicU64::new(1);

pub struct Settings {
    /// The `Kodosi Screen` app on a Mac, its program elsewhere.
    pub helper: PathBuf,
    /// On a Mac, a private folder with a short path for the helper's socket.
    pub sockets: PathBuf,
    /// Passed on to the helper.
    pub arguments: Vec<OsString>,
    /// Tells the person at the computer who watches.
    pub notice: Arc<dyn Fn(&[String]) -> Notice + Send + Sync>,
    /// Called when the person at the computer stops the sharing there.
    pub stopped: Arc<dyn Fn() + Send + Sync>,
}

#[derive(Debug)]
pub enum Error {
    Helper(String),
    Stopped,
    Malformed,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Helper(problem) => write!(formatter, "the screen helper failed: {problem}"),
            Self::Stopped => formatter.write_str("the screen stopped"),
            Self::Malformed => formatter.write_str("a message that does not read"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Helper(error.to_string())
    }
}

/// The computer's screen for every terminal on it: one helper and one capture, which all
/// viewers share.
#[derive(Clone)]
pub struct Screens {
    shared: Arc<Shared>,
}

struct Shared {
    settings: Settings,
    live: Mutex<Option<mpsc::Sender<Event>>>,
    wanted: std::sync::Mutex<Wanted>,
}

impl Screens {
    pub fn new(settings: Settings) -> Self {
        Self {
            shared: Arc::new(Shared {
                settings,
                live: Mutex::new(None),
                wanted: std::sync::Mutex::new(Wanted::Screen),
            }),
        }
    }

    pub async fn watch(&self, viewer: Viewer) -> Result<Pipe, Error> {
        let events = self.live().await?;
        let (outgoing, incoming) = mpsc::channel(VIEWER_QUEUE);
        let mailbox = Arc::new(Mailbox::default());
        let seat = SEATS.fetch_add(1, Ordering::Relaxed);
        let id = viewer.id.clone();
        events
            .send(Event::Join {
                viewer,
                seat,
                outgoing,
                mailbox: Arc::clone(&mailbox),
            })
            .await
            .map_err(|_| Error::Stopped)?;
        Ok(Pipe {
            viewer: id,
            seat,
            incoming,
            mailbox,
            events,
        })
    }

    /// Shows a window whose app or title holds `wanted`'s words on every view, or the main
    /// display.
    pub async fn show(&self, wanted: Wanted) {
        *self
            .shared
            .wanted
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = wanted.clone();
        let live = self.shared.live.lock().await.clone();
        if let Some(live) = live {
            drop(live.send(Event::Show(wanted)).await);
        }
    }

    /// What macOS lets the helper do, which starts it when it is not running.
    pub async fn access(&self) -> Result<Access, Error> {
        let events = self.live().await?;
        let (reply, access) = oneshot::channel();
        events
            .send(Event::Access(reply))
            .await
            .map_err(|_| Error::Stopped)?;
        access.await.map_err(|_| Error::Stopped)
    }

    /// Shows the person at the computer macOS's prompts for what the helper may not do yet.
    pub async fn ask(&self) -> Result<(), Error> {
        let events = self.live().await?;
        events.send(Event::Ask).await.map_err(|_| Error::Stopped)
    }

    /// Ends every view and quits the helper.
    pub async fn stop(&self) {
        let live = self.shared.live.lock().await.take();
        if let Some(live) = live {
            drop(live.send(Event::Stop).await);
        }
    }

    async fn live(&self) -> Result<mpsc::Sender<Event>, Error> {
        let mut live = self.shared.live.lock().await;
        if let Some(events) = live.as_ref().filter(|events| !events.is_closed()) {
            return Ok(events.clone());
        }
        let started = start(&self.shared.settings).await?;
        let (events, receiver) = mpsc::channel(EVENTS);
        let wanted = self
            .shared
            .wanted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let calls = Calls {
            notice: Arc::clone(&self.shared.settings.notice),
            stopped: Arc::clone(&self.shared.settings.stopped),
        };
        let (session, reader) = Live::new(started, wanted, calls);
        tokio::spawn(session.run(reader, receiver));
        *live = Some(events.clone());
        drop(live);
        Ok(events)
    }
}

/// The helper's end of the link: a socket on a Mac, the helper's own pipes elsewhere.
pub(crate) type Reading = Box<dyn AsyncRead + Send + Unpin>;
pub(crate) type Writing = Box<dyn AsyncWrite + Send + Unpin>;

/// A connected helper and what it said first.
pub(crate) struct Started {
    pub(crate) reader: Reading,
    pub(crate) writer: Writing,
    /// The helper where the host runs it as its child, which ends with the screen.
    pub(crate) child: Option<tokio::process::Child>,
    pub(crate) encodable: Vec<stream::Codec>,
    pub(crate) largest: (u32, u32),
    pub(crate) access: Access,
    pub(crate) away: Option<Away>,
}

/// Opens the helper through Launch Services, so that macOS asks for its permissions in its own
/// name rather than in the name of the app that started the host.
#[cfg(target_os = "macos")]
async fn start(settings: &Settings) -> Result<Started, Error> {
    let token = token()?;
    tokio::fs::create_dir_all(&settings.sockets).await?;
    let path = settings
        .sockets
        .join(format!("screen-{}.sock", &token[..12]));
    drop(tokio::fs::remove_file(&path).await);
    let listener = tokio::net::UnixListener::bind(&path)?;
    let opened = Command::new("/usr/bin/open")
        .args(["-g", "-n", "-a"])
        .arg(&settings.helper)
        .arg("--args")
        .arg("--link")
        .arg(&path)
        .arg("--token")
        .arg(&token)
        .args(&settings.arguments)
        .status()
        .await;
    let accepted = match opened {
        Ok(status) if status.success() => tokio::time::timeout(CONNECT_LIMIT, listener.accept())
            .await
            .map_err(|_| Error::Helper("it did not connect".to_owned()))
            .and_then(|accepted| accepted.map_err(Error::from)),
        Ok(status) => Err(Error::Helper(format!("it did not open: {status}"))),
        Err(error) => Err(error.into()),
    };
    drop(listener);
    drop(tokio::fs::remove_file(&path).await);
    let (reader, writer) = accepted?.0.into_split();
    hello(Box::new(reader), Box::new(writer), None, &token).await
}

/// Runs the helper as the host's child, linked through its standard input and output.
#[cfg(not(target_os = "macos"))]
async fn start(settings: &Settings) -> Result<Started, Error> {
    let token = token()?;
    let mut child = Command::new(&settings.helper)
        .arg("--token")
        .arg(&token)
        .args(&settings.arguments)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let (Some(writer), Some(reader)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(Error::Helper("it has no pipes".to_owned()));
    };
    hello(Box::new(reader), Box::new(writer), Some(child), &token).await
}

async fn hello(
    mut reader: Reading,
    writer: Writing,
    child: Option<tokio::process::Child>,
    token: &str,
) -> Result<Started, Error> {
    let hello = tokio::time::timeout(HELLO_LIMIT, link::read::<FromHelper>(&mut reader))
        .await
        .map_err(|_| Error::Helper("it said nothing".to_owned()))??;
    match hello {
        Some(Message::Said(FromHelper::Hello {
            token: said,
            version,
            encodable,
            largest,
            access,
            away,
        })) if said == token && version == VERSION => Ok(Started {
            reader,
            writer,
            child,
            encodable,
            largest,
            access,
            away,
        }),
        _ => Err(Error::Helper("it is another version".to_owned())),
    }
}

fn token() -> io::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)?;
    let mut text = String::with_capacity(64);
    for byte in bytes {
        write!(text, "{byte:02x}").map_err(io::Error::other)?;
    }
    Ok(text)
}

pub struct Pipe {
    viewer: String,
    seat: u64,
    incoming: mpsc::Receiver<Bytes>,
    mailbox: Arc<Mailbox>,
    events: mpsc::Sender<Event>,
}

impl Pipe {
    pub async fn send(&self, message: &[u8]) -> Result<(), Error> {
        let message = viewer::from_json::<ToScreen>(message).ok_or(Error::Malformed)?;
        self.events
            .send(Event::Message {
                viewer: self.viewer.clone(),
                message,
            })
            .await
            .map_err(|_| Error::Stopped)
    }

    /// Returns the next message for the viewer, the newest state before waiting frames.
    pub async fn receive(&mut self) -> Option<Bytes> {
        loop {
            if let Some(state) = self.mailbox.take() {
                return Some(viewer::json(&state));
            }
            tokio::select! {
                biased;
                () = self.mailbox.ready.notified() => {}
                picture = self.incoming.recv() => return picture,
            }
        }
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        let leave = Event::Leave {
            viewer: std::mem::take(&mut self.viewer),
            seat: self.seat,
        };
        // A lost leave would keep the screen captured and encoded for a viewer who is gone.
        if let Err(mpsc::error::TrySendError::Full(leave)) = self.events.try_send(leave) {
            let events = self.events.clone();
            tokio::spawn(async move { drop(events.send(leave).await) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_viewer_leaves_even_while_the_screen_is_behind() {
        let (events, mut heard) = mpsc::channel(1);
        events.try_send(Event::Stop).unwrap();
        let (_, incoming) = mpsc::channel(1);
        drop(Pipe {
            viewer: "viewer".to_owned(),
            seat: 3,
            incoming,
            mailbox: Arc::default(),
            events,
        });
        assert!(matches!(heard.recv().await, Some(Event::Stop)));
        assert!(matches!(
            heard.recv().await,
            Some(Event::Leave { seat: 3, .. })
        ));
    }
}
