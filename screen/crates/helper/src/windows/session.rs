//! The link to the host: what it asks of the capture, the input that it passes on, and what the
//! helper tells it back.

#![expect(
    clippy::future_not_send,
    reason = "the link's thread runs its own futures, with Windows handles that stay there"
)]

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use screen::link::{self, Access, Away, FromHelper, Message, Source, Sources, ToHelper, VERSION};
use stream::viewer::{Input, KeyAction, MouseAction};
use stream_encode::{Encoder, Settings};
use tokio::{io::AsyncWriteExt as _, sync::mpsc};

use super::{
    app::Window,
    capture::{Capture, Frames, Wanted},
    clipboard::{self, Clipboard},
    hand::{self, Listening},
    input::Hands,
    power::{self, Power},
    sources::{self, Screen},
};

/// Messages on their way to the host: frames, the pointer and what the helper says.
pub(super) const MESSAGES: usize = 64;
/// How often the helper looks again at the displays and whether they can be seen.
const LOOK_EVERY: Duration = Duration::from_secs(2);

pub(super) enum Event {
    /// The person at the computer stopped the sharing.
    Stopped,
    /// The capture or its encoder failed.
    Failed(String),
    /// The session locked or unlocked or changed hands, or the displays slept, woke or changed.
    Changed,
    /// Something was copied on the computer.
    ClipboardChanged,
    /// Text that someone copied on the computer.
    Copied(String),
}

/// Serves the host on standard input and output until it quits or goes away.
pub(super) fn serve(
    token: String,
    window: Window,
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
        tokio::spawn(write(saying));
        let mut session = Session::new(window, say, events);
        session.hello(token).await;
        session.run(hearing).await;
    });
    // Reading the host's link blocks a thread, which would keep the helper from quitting.
    runtime.shutdown_timeout(Duration::ZERO);
}

async fn write(mut saying: mpsc::Receiver<Bytes>) {
    let mut output = tokio::io::stdout();
    while let Some(message) = saying.recv().await {
        if output.write_all(&message).await.is_err() || output.flush().await.is_err() {
            return;
        }
    }
}

async fn read(heard: mpsc::Sender<ToHelper>) {
    let mut input = tokio::io::stdin();
    while let Ok(Some(message)) = link::read::<ToHelper>(&mut input).await {
        if let Message::Said(said) = message
            && heard.send(said).await.is_err()
        {
            return;
        }
    }
}

struct Session {
    window: Window,
    say: mpsc::Sender<Bytes>,
    events: mpsc::UnboundedSender<Event>,
    away: Option<Away>,
    /// The display slept at the last look, which woke it while someone watched.
    slept: bool,
    watched: bool,
    power: Power,
    /// The displays as the helper last looked, where viewers' input goes.
    screens: Vec<Screen>,
    sources: Sources,
    capture: Option<Capture>,
    /// The encoder, from the first capture until the host stops it.
    frames: Option<Arc<Frames>>,
    hands: Hands,
    /// Hears the person at the computer while the screen is captured.
    listening: Option<Listening>,
    clipboard: Clipboard,
}

impl Session {
    fn new(window: Window, say: mpsc::Sender<Bytes>, events: mpsc::UnboundedSender<Event>) -> Self {
        Self {
            window,
            say,
            events,
            away: None,
            slept: false,
            watched: false,
            power: Power::default(),
            screens: Vec::new(),
            sources: Sources::default(),
            capture: None,
            frames: None,
            hands: Hands::new(),
            listening: None,
            clipboard: Clipboard::new(),
        }
    }

    /// Windows asks nothing of the person at the computer before the helper records the screen
    /// or plays input.
    async fn hello(&mut self, token: String) {
        self.away = self.away_now();
        self.tell(FromHelper::Hello {
            token,
            version: VERSION,
            encodable: Encoder::encodable(),
            largest: Encoder::largest(),
            access: Access {
                screen: true,
                input: true,
            },
            away: self.away,
        })
        .await;
        self.look().await;
    }

    async fn run(&mut self, mut events: mpsc::UnboundedReceiver<Event>) {
        let (heard, mut hearing) = mpsc::channel(MESSAGES);
        tokio::spawn(read(heard));
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
                _ = looking.tick() => self.look().await,
            }
        }
        self.stop().await;
        self.window.notice(None);
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
            ToHelper::Input { input } => self.play(&input),
            ToHelper::Notice { notice } => {
                let first = notice.is_some() && !self.watched;
                self.watched = notice.is_some();
                self.window.notice(notice);
                // Opening the screen wakes its display.
                if first {
                    self.slept = false;
                    self.look().await;
                }
            }
            ToHelper::Clipboard { text } => self.clipboard.put(self.window, &text),
            ToHelper::Ask => {}
            ToHelper::Quit => return false,
        }
        true
    }

    async fn capture(&mut self, wanted: Wanted, settings: Settings) -> Result<(), String> {
        let Source::Display { id } = wanted.source else {
            return Err("Kodosi Screen shares displays on Windows, not windows".to_owned());
        };
        let screen = self
            .screens
            .iter()
            .find(|screen| screen.id == id)
            .cloned()
            .ok_or("the display is gone")?;
        let frames = if let Some(frames) = &self.frames {
            Arc::clone(frames)
        } else {
            let frames = Frames::new(settings, self.say.clone(), self.events.clone());
            self.frames = Some(Arc::clone(&frames));
            frames
        };
        frames.encode_as(settings);
        if let Some(capture) = self
            .capture
            .as_ref()
            .filter(|capture| capture.source == wanted.source)
        {
            capture.update(wanted);
            return Ok(());
        }
        if let Some(earlier) = self.capture.take() {
            earlier.stop().await;
        } else {
            self.clipboard.settle();
        }
        self.capture = Some(
            Capture::start(
                screen,
                wanted,
                frames,
                self.say.clone(),
                self.events.clone(),
            )
            .await?,
        );
        self.power.keep_awake(true);
        if self.listening.is_none() {
            self.listening = Listening::start();
        }
        Ok(())
    }

    async fn stop(&mut self) {
        if let Some(capture) = self.capture.take() {
            capture.stop().await;
        }
        self.listening = None;
        self.power.keep_awake(false);
        self.frames = None;
    }

    /// Stops the capture after it failed; the host asks again, maybe for another display.
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
            Event::Changed => self.look().await,
            Event::ClipboardChanged => self.copies(),
            Event::Copied(text) => {
                if self.clipboard.is_new(&text) {
                    self.tell(FromHelper::Copied { text }).await;
                }
            }
        }
    }

    fn encoder(&self, work: impl FnOnce(&mut Encoder)) {
        if let Some(frames) = &self.frames {
            frames.encoder(work);
        }
    }

    /// Reads what was copied on the computer while the screen is captured, which may wait for the
    /// program that copied it to give it.
    fn copies(&mut self) {
        if self.capture.is_none() || !self.clipboard.changed() {
            return;
        }
        let events = self.events.clone();
        tokio::spawn(async move {
            if let Ok(Some(text)) = tokio::task::spawn_blocking(clipboard::copied).await {
                drop(events.send(Event::Copied(text)));
            }
        });
    }

    fn play(&mut self, input: &Input) {
        let Some(source) = self.capture.as_ref().map(|capture| capture.source) else {
            return;
        };
        if hand::busy() && !lets_go(input) {
            return;
        }
        if let Some(screen) = self
            .screens
            .iter()
            .find(|screen| Source::Display { id: screen.id } == source)
        {
            self.hands.play(input, screen);
        }
    }

    /// Tells the host what changed, the sources before the screen comes back, so that it
    /// captures what is there now.
    async fn look(&mut self) {
        let away = self.away_now();
        if away.is_none() {
            let screens = sources::screens();
            if !screens.is_empty() {
                let sources = sources::sources(&screens);
                self.screens = screens;
                if sources != self.sources {
                    self.sources = sources.clone();
                    self.tell(FromHelper::Sources { sources }).await;
                }
            }
        }
        if away != self.away {
            self.away = away;
            self.tell(FromHelper::Away { away }).await;
        }
    }

    /// A display that sleeps while someone watches wakes, and is away only when it sleeps on.
    fn away_now(&mut self) -> Option<Away> {
        if let Some(away) = power::session_away() {
            self.slept = false;
            return Some(away);
        }
        let asleep = power::display_asleep();
        let slept = std::mem::replace(&mut self.slept, asleep);
        if asleep && self.watched {
            power::wake();
        }
        (asleep && slept).then_some(Away::Asleep)
    }

    async fn tell(&self, message: FromHelper) {
        drop(self.say.send(link::json(&message)).await);
    }
}

/// Letting go of what a viewer holds goes through even while the person at the computer uses it.
const fn lets_go(input: &Input) -> bool {
    matches!(
        input,
        Input::Mouse {
            action: MouseAction::Release,
            ..
        } | Input::Key {
            action: KeyAction::Up,
            ..
        }
    )
}
