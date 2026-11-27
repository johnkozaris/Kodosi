//! The link to the host: what it asks of the capture, the input that it passes on, and what the
//! helper tells it back.

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{
    CGPreflightPostEventAccess, CGPreflightScreenCaptureAccess, CGRequestPostEventAccess,
    CGRequestScreenCaptureAccess,
};
use screen::{
    handover,
    link::{self, Access, Away, FromHelper, Message, Question, Source, Sources, ToHelper, VERSION},
    signature::Signer,
};
use stream::viewer::{Input, KeyAction, MouseAction};
use stream_encode::{Encoder, Settings};
use tokio::{
    io::AsyncWriteExt as _,
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::mpsc,
};

use super::{
    alert,
    answers::Answers,
    app,
    capture::{Capture, Frames, Wanted},
    clipboard::{self, Clipboard},
    gate::{Gate, Step},
    hand,
    input::Hands,
    power::{self, Power},
    raise,
    sources::{self, Content},
};

/// Messages on their way to the host: frames, the pointer and what the helper says.
pub(super) const MESSAGES: usize = 64;
/// How often the helper looks again at the screens, the windows and its permissions.
const LOOK_EVERY: Duration = Duration::from_secs(2);
/// How long a raised window takes to come to the front, before the click that follows.
const RAISING: Duration = Duration::from_millis(50);
/// How often the helper looks whether something was copied, while the screen is captured.
const COPIES_EVERY: Duration = Duration::from_millis(500);
/// The host hands over the link once Launch Services says that the helper opened.
const HANDOVER_LIMIT: Duration = Duration::from_secs(15);
/// Kodosi Screen's bundle, which after the team names the keychain group of its signature.
const BUNDLE: &str = "com.kodosi.screen";
/// Kodosi's app offers its vouch before it sends the click on to the host, so it answers at once.
const VOUCH_LIMIT: Duration = Duration::from_secs(5);

pub(super) enum Event {
    /// The person at the computer stopped the sharing.
    Stopped,
    /// The capture or its encoder failed.
    Failed(String),
    /// Text that someone copied on the computer.
    Copied(String),
    /// The screen locked or unlocked, the displays slept or woke, or the account on the display
    /// changed.
    Changed,
    /// The person at the computer answered alert `alert`, or it could not show.
    Answered {
        alert: u64,
        answer: Result<bool, String>,
    },
}

/// Takes the link from the host and serves it until the link closes, or quits before it says
/// anything when another program handed the link over.
pub(super) fn serve(
    link: &str,
    token: String,
    say: mpsc::Sender<Bytes>,
    saying: mpsc::Receiver<Bytes>,
    events: mpsc::UnboundedSender<Event>,
    hearing: mpsc::UnboundedReceiver<Event>,
) {
    let Some(signer) = Signer::own() else {
        return;
    };
    let Ok(link) = handover::take(link, HANDOVER_LIMIT, &signer) else {
        return;
    };
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    runtime.block_on(async move {
        let link = std::os::unix::net::UnixStream::from(link);
        let Ok(socket) = link
            .set_nonblocking(true)
            .and_then(|()| UnixStream::from_std(link))
        else {
            return;
        };
        let (reader, writer) = socket.into_split();
        tokio::spawn(write(writer, saying));
        let mut session = Session::new(say, events, signer);
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
    gate: Gate,
    /// The team that signed this program, whose app vouches for the person's clicks.
    signer: Signer,
    /// Where a Kodosi Screen of a team keeps the accounts that the person allowed.
    answers: Option<Answers>,
    access: Access,
    away: Option<Away>,
    /// The display slept at the last look, which woke it while someone watched.
    slept: bool,
    watched: bool,
    power: Power,
    content: Option<Content>,
    sources: Sources,
    capture: Option<Capture>,
    /// The encoder, from the first capture until the host stops it.
    frames: Option<Arc<Frames>>,
    hands: Hands,
    clipboard: Clipboard,
}

impl Session {
    fn new(say: mpsc::Sender<Bytes>, events: mpsc::UnboundedSender<Event>, signer: Signer) -> Self {
        let answers = match &signer {
            Signer::Team(team) => Some(Answers::new(&format!("{team}.{BUNDLE}"))),
            Signer::NoTeam => None,
        };
        Self {
            say,
            events,
            gate: Gate::new(answers.is_some()),
            signer,
            answers,
            access: Access::default(),
            away: None,
            slept: false,
            watched: false,
            power: Power::default(),
            content: None,
            sources: Sources::default(),
            capture: None,
            frames: None,
            hands: Hands::new(),
            clipboard: Clipboard::new(),
        }
    }

    async fn hello(&mut self, token: String) {
        self.access = access();
        self.away = self.away_now();
        self.tell(FromHelper::Hello {
            token,
            version: VERSION,
            encodable: Encoder::encodable(),
            largest: Encoder::largest(),
            access: self.access,
            away: self.away,
        })
        .await;
        self.look().await;
    }

    async fn run(&mut self, reader: OwnedReadHalf, mut events: mpsc::UnboundedReceiver<Event>) {
        let (heard, mut hearing) = mpsc::channel(MESSAGES);
        tokio::spawn(read(reader, heard));
        let mut looking = tokio::time::interval(LOOK_EVERY);
        looking.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut copies = tokio::time::interval(COPIES_EVERY);
        copies.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
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
                _ = copies.tick(), if self.capture.is_some() => self.copies(),
            }
        }
        self.stop().await;
    }

    /// Returns false when the host wants the helper to quit.
    async fn hear(&mut self, said: ToHelper) -> bool {
        if !self.gate.admits(&said) {
            return true;
        }
        match said {
            ToHelper::Capture {
                source,
                region,
                width,
                height,
                format,
                bitrate,
                pace,
                ..
            } => {
                let wanted = Wanted {
                    source,
                    region,
                    width,
                    height,
                };
                let settings = Settings {
                    format,
                    bitrate,
                    pace,
                };
                if let Err(problem) = self.capture(wanted, settings).await {
                    self.fail(problem).await;
                }
            }
            ToHelper::Stop => self.stop().await,
            ToHelper::Keyframe => self.encoder(Encoder::want_key),
            ToHelper::Recover => self.encoder(Encoder::recover),
            ToHelper::Held { marks } => self.encoder(|encoder| encoder.held(&marks)),
            ToHelper::Bitrate { bitrate } => self.encoder(|encoder| encoder.set_bitrate(bitrate)),
            ToHelper::Pace { pace } => self.encoder(|encoder| encoder.set_pace(pace)),
            ToHelper::Input { input } => self.play(&input).await,
            ToHelper::Notice { notice } => {
                let first = notice.is_some() && !self.watched;
                self.watched = notice.is_some();
                app::notice(notice);
                // Opening the screen wakes its display.
                if first {
                    self.slept = false;
                    self.look().await;
                }
            }
            ToHelper::Clipboard { text } => self.clipboard.put(&text),
            ToHelper::Ask => {
                if !self.access.screen {
                    CGRequestScreenCaptureAccess();
                }
                if !self.access.input {
                    CGRequestPostEventAccess();
                }
            }
            ToHelper::Owner { key } => self.own(key).await,
            ToHelper::Allow { question, vouch } => self.allow(question, vouch.as_deref()).await,
            ToHelper::Forget => self.forget().await,
            ToHelper::Quit => return false,
        }
        true
    }

    async fn own(&mut self, key: String) {
        let answers = self.answers.as_ref();
        let known = |key: &str| answers.is_some_and(|answers| answers.known(key));
        if let Some(allowed) = self.gate.own(key, known) {
            self.tell(FromHelper::Owner { allowed }).await;
            if allowed {
                self.look().await;
            }
        }
    }

    async fn allow(&mut self, question: Question, vouch: Option<&str>) {
        let mut unvouched = None;
        let signer = &self.signer;
        let vouched = || {
            let taken = vouch
                .ok_or_else(|| "no vouch came with the ask".to_owned())
                .and_then(|vouch| {
                    handover::vouched(vouch, VOUCH_LIMIT, signer).map_err(|error| error.to_string())
                });
            taken
                .map_err(|problem| {
                    unvouched = Some(format!(
                        "Kodosi Screen asks only for a click in Kodosi's app: {problem}"
                    ));
                })
                .is_ok()
        };
        match self.gate.ask(vouched) {
            Step::Answer(allowed) => {
                self.tell(FromHelper::Allowed {
                    allowed,
                    problem: unvouched,
                })
                .await;
            }
            Step::Join => {}
            Step::Show(shown) => match alert::intact() {
                Ok(()) => alert::show(question, shown, self.events.clone()),
                Err(problem) => self.answer(shown, Err(problem)).await,
            },
        }
    }

    /// Keeps the account right after the person allowed it in this program's alert, and answers
    /// each ask that waited for it. Without a keeping the answer is no, as the host learns why.
    async fn answer(&mut self, shown: u64, answer: Result<bool, String>) {
        let Some(key) = self.gate.standing(shown) else {
            return;
        };
        let kept = match (answer, &self.answers) {
            (Ok(true), Some(answers)) => answers.keep(key).map(|()| true),
            (answer, _) => answer,
        };
        let (allowed, problem) = match kept {
            Ok(allowed) => (allowed, None),
            Err(problem) => (false, Some(problem)),
        };
        for _ in 0..self.gate.settle(allowed) {
            self.tell(FromHelper::Allowed {
                allowed,
                problem: problem.clone(),
            })
            .await;
        }
        if allowed {
            self.look().await;
        }
    }

    /// Off means nobody: every account that the person allowed needs their answer again.
    async fn forget(&mut self) {
        // The keychain refuses only a helper that cannot read the answers either.
        if let Some(answers) = &self.answers {
            drop(answers.forget());
        }
        for _ in 0..self.gate.forget() {
            self.tell(FromHelper::Allowed {
                allowed: false,
                problem: None,
            })
            .await;
        }
        self.stop().await;
        app::notice(None);
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
            return capture.update(content, wanted).await;
        }
        if let Some(earlier) = self.capture.take() {
            earlier.stop().await;
        } else {
            self.clipboard.settle();
        }
        self.capture = Some(Capture::start(content, wanted, frames).await?);
        self.power.keep_awake(true);
        app::follow(Some(wanted.source));
        Ok(())
    }

    async fn stop(&mut self) {
        app::follow(None);
        self.hands.let_go();
        if let Some(capture) = self.capture.take() {
            capture.stop().await;
        }
        self.power.keep_awake(false);
        self.frames = None;
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
                self.forget().await;
                self.tell(FromHelper::Stopped).await;
            }
            Event::Answered { alert, answer } => self.answer(alert, answer).await,
            Event::Failed(problem) => {
                if self.capture.is_some() {
                    self.fail(problem).await;
                }
            }
            Event::Changed => self.look().await,
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

    /// Reads what was copied on the computer, which may wait for the person at the computer to
    /// allow it.
    fn copies(&mut self) {
        if !self.clipboard.changed() {
            return;
        }
        let events = self.events.clone();
        tokio::spawn(async move {
            if let Ok(Some(text)) = tokio::task::spawn_blocking(clipboard::copied).await {
                drop(events.send(Event::Copied(text)));
            }
        });
    }

    async fn play(&mut self, input: &Input) {
        let Some(source) = self.capture.as_ref().map(|capture| capture.source) else {
            return;
        };
        if hand::busy() && !lets_go(input) {
            return;
        }
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

    /// Tells the host what changed, the sources before the screen comes back, so that it
    /// captures what is there now.
    async fn look(&mut self) {
        self.check_access().await;
        let away = self.away_now();
        if away.is_none()
            && self.access.screen
            && self.gate.serves()
            && let Ok(content) = Content::now().await
        {
            let sources = content.sources();
            self.content = Some(content);
            if !same(&sources, &self.sources) {
                self.sources = sources.clone();
                self.tell(FromHelper::Sources { sources }).await;
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
            self.power.wake();
        }
        (asleep && slept).then_some(Away::Asleep)
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
