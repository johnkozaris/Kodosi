//! The link to the host: what it asks of the capture, the input that it passes on, and what the
//! helper tells it back.

use std::{
    future,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use bytes::Bytes;
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{
    CGPreflightPostEventAccess, CGPreflightScreenCaptureAccess, CGRequestPostEventAccess,
    CGRequestScreenCaptureAccess,
};
use screen::link::{self, Access, FromHelper, Message, Source, Sources, ToHelper, VERSION};
use stream::viewer::{Input, MouseAction};
use stream_encode::{
    Encoder, Settings,
    refine::{REFINE_MS, STILL_MS},
};
use tokio::{
    io::AsyncWriteExt as _,
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{mpsc, watch},
};

use super::{
    app,
    capture::{Capture, Frames, Wanted},
    input::Hands,
    raise,
    sources::{self, Content},
};

/// Messages on their way to the host: frames, the pointer and what the helper says.
pub(super) const MESSAGES: usize = 64;
/// How often the helper looks again at the screens, the windows and its permissions.
const LOOK_EVERY: Duration = Duration::from_secs(2);
/// How long a raised window takes to come to the front, before the click that follows.
const RAISING: Duration = Duration::from_millis(50);

pub(super) enum Event {
    /// The person at the computer stopped the sharing.
    Stopped,
    /// The capture or its encoder failed.
    Failed(String),
}

/// Connects to the host and serves it until the link closes.
pub(super) fn serve(
    link: &Path,
    token: String,
    say: mpsc::Sender<Bytes>,
    saying: mpsc::Receiver<Bytes>,
    events: mpsc::UnboundedSender<Event>,
    hearing: mpsc::UnboundedReceiver<Event>,
) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    runtime.block_on(async move {
        let Ok(socket) = UnixStream::connect(link).await else {
            return;
        };
        let (reader, writer) = socket.into_split();
        tokio::spawn(write(writer, saying));
        let mut session = Session::new(say, events);
        session.hello(token).await;
        session.run(reader, hearing).await;
    });
}

async fn write(mut writer: OwnedWriteHalf, mut saying: mpsc::Receiver<Bytes>) {
    while let Some(message) = saying.recv().await {
        if writer.write_all(&message).await.is_err() {
            return;
        }
    }
}

async fn read(mut reader: OwnedReadHalf, heard: mpsc::Sender<ToHelper>) {
    while let Ok(Some(message)) = link::read::<ToHelper>(&mut reader).await {
        if let Message::Said(said) = message
            && heard.send(said).await.is_err()
        {
            return;
        }
    }
}

struct Session {
    say: mpsc::Sender<Bytes>,
    events: mpsc::UnboundedSender<Event>,
    access: Access,
    content: Option<Content>,
    sources: Sources,
    capture: Option<Capture>,
    /// The encoder, from the first capture until the host stops it.
    frames: Option<Arc<Frames>>,
    drawn: Option<watch::Receiver<u64>>,
    /// When to sharpen which picture next.
    refine: Option<(Instant, u64)>,
    hands: Hands,
}

impl Session {
    fn new(say: mpsc::Sender<Bytes>, events: mpsc::UnboundedSender<Event>) -> Self {
        Self {
            say,
            events,
            access: Access::default(),
            content: None,
            sources: Sources::default(),
            capture: None,
            frames: None,
            drawn: None,
            refine: None,
            hands: Hands::new(),
        }
    }

    async fn hello(&mut self, token: String) {
        self.access = access();
        self.tell(FromHelper::Hello {
            token,
            version: VERSION,
            encodable: Encoder::encodable(),
            access: self.access,
        })
        .await;
        self.look().await;
    }

    async fn run(&mut self, reader: OwnedReadHalf, mut events: mpsc::UnboundedReceiver<Event>) {
        let (heard, mut hearing) = mpsc::channel(MESSAGES);
        tokio::spawn(read(reader, heard));
        let mut looking = tokio::time::interval(LOOK_EVERY);
        looking.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                said = hearing.recv() => {
                    let Some(said) = said else {
                        break;
                    };
                    if !self.hear(said).await {
                        break;
                    }
                }
                Some(event) = events.recv() => self.event(event).await,
                Some(picture) = drawn(self.drawn.as_mut()) => {
                    self.refine = Some((Instant::now() + millis(STILL_MS), picture));
                }
                () = until(self.refine.map(|(at, _)| at)) => self.sharpen(),
                _ = looking.tick() => self.look().await,
            }
        }
        self.stop().await;
    }

    /// Returns false when the host wants the helper to quit.
    async fn hear(&mut self, said: ToHelper) -> bool {
        match said {
            ToHelper::Capture {
                source,
                region,
                width,
                height,
                format,
                bitrate,
            } => {
                let wanted = Wanted {
                    source,
                    region,
                    width,
                    height,
                };
                if let Err(problem) = self.capture(wanted, Settings { format, bitrate }).await {
                    self.fail(problem).await;
                }
            }
            ToHelper::Stop => self.stop().await,
            ToHelper::Keyframe => self.encoder(Encoder::want_key),
            ToHelper::Recover => self.encoder(Encoder::recover),
            ToHelper::Held { marks } => self.encoder(|encoder| encoder.held(&marks)),
            ToHelper::Bitrate { bitrate } => self.encoder(|encoder| encoder.set_bitrate(bitrate)),
            ToHelper::Input { input } => self.play(&input).await,
            ToHelper::Notice { notice } => app::notice(notice),
            ToHelper::Ask => {
                if !self.access.screen {
                    CGRequestScreenCaptureAccess();
                }
                if !self.access.input {
                    CGRequestPostEventAccess();
                }
            }
            ToHelper::Quit => return false,
        }
        true
    }

    async fn capture(&mut self, wanted: Wanted, settings: Settings) -> Result<(), String> {
        if !self.access.screen {
            return Err("Kodosi Screen may not record the screen".to_owned());
        }
        let content = self
            .content
            .as_ref()
            .ok_or("Kodosi Screen has not listed the screens yet")?;
        let frames = if let Some(frames) = &self.frames {
            Arc::clone(frames)
        } else {
            let (frames, drawn) = Frames::new(settings, self.say.clone(), self.events.clone());
            self.frames = Some(Arc::clone(&frames));
            self.drawn = Some(drawn);
            frames
        };
        frames.encode_as(settings);
        if let Some(capture) = self
            .capture
            .as_ref()
            .filter(|capture| capture.source == wanted.source)
        {
            return capture.update(content, wanted).await;
        }
        if let Some(earlier) = self.capture.take() {
            earlier.stop().await;
        }
        self.capture = Some(Capture::start(content, wanted, frames).await?);
        app::follow(Some(wanted.source));
        Ok(())
    }

    async fn stop(&mut self) {
        app::follow(None);
        if let Some(capture) = self.capture.take() {
            capture.stop().await;
        }
        self.frames = None;
        self.drawn = None;
        self.refine = None;
    }

    /// Stops the capture after it failed; the host asks again, maybe for another source.
    async fn fail(&mut self, problem: String) {
        self.stop().await;
        self.tell(FromHelper::Failed { problem }).await;
        self.look().await;
    }

    async fn event(&mut self, event: Event) {
        match event {
            Event::Stopped => {
                self.stop().await;
                self.tell(FromHelper::Stopped).await;
            }
            Event::Failed(problem) => {
                if self.capture.is_some() {
                    self.fail(problem).await;
                }
            }
        }
    }

    fn encoder(&self, work: impl FnOnce(&mut Encoder)) {
        if let Some(frames) = &self.frames {
            frames.encoder(work);
        }
    }

    fn sharpen(&mut self) {
        let Some((_, picture)) = self.refine.take() else {
            return;
        };
        if self
            .frames
            .as_ref()
            .is_some_and(|frames| frames.refine(picture))
        {
            self.refine = Some((Instant::now() + millis(REFINE_MS), picture));
        }
    }

    async fn play(&mut self, input: &Input) {
        let Some(source) = self.capture.as_ref().map(|capture| capture.source) else {
            return;
        };
        let Some(bounds) = sources::bounds(source).filter(|_| self.access.input) else {
            return;
        };
        if let (
            Source::Window { id },
            Input::Mouse {
                action: MouseAction::Press,
                x,
                y,
                ..
            },
        ) = (source, input)
            && raise::raise(id, CGPoint::new(bounds.origin.x + x, bounds.origin.y + y))
        {
            tokio::time::sleep(RAISING).await;
        }
        self.hands.play(input, bounds);
    }

    async fn look(&mut self) {
        self.check_access().await;
        if !self.access.screen {
            return;
        }
        let Ok(content) = Content::now().await else {
            return;
        };
        let sources = content.sources();
        self.content = Some(content);
        if !same(&sources, &self.sources) {
            self.sources = sources.clone();
            self.tell(FromHelper::Sources { sources }).await;
        }
    }

    async fn check_access(&mut self) {
        let access = access();
        if access != self.access {
            self.access = access;
            self.tell(FromHelper::Access { access }).await;
        }
    }

    async fn tell(&self, message: FromHelper) {
        drop(self.say.send(link::json(&message)).await);
    }
}

/// Windows come front to back, so bringing one forward changes their order but not the sources.
fn same(sources: &Sources, known: &Sources) -> bool {
    let by_id = |sources: &Sources| {
        let mut windows = sources.windows.clone();
        windows.sort_by_key(|window| window.id);
        windows
    };
    sources.displays == known.displays && by_id(sources) == by_id(known)
}

fn access() -> Access {
    Access {
        screen: CGPreflightScreenCaptureAccess(),
        input: CGPreflightPostEventAccess(),
    }
}

async fn drawn(drawn: Option<&mut watch::Receiver<u64>>) -> Option<u64> {
    let drawn = drawn?;
    drawn.changed().await.ok()?;
    Some(*drawn.borrow_and_update())
}

async fn until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at.into()).await,
        None => future::pending().await,
    }
}

fn millis(millis: i64) -> Duration {
    Duration::from_millis(u64::try_from(millis).unwrap_or_default())
}
