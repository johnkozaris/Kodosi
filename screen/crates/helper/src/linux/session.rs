//! The link to the host: what it asks of the capture, the input that it passes on, and what the
//! helper tells it back. The desktop shares the screen in a portal session, which starts when
//! someone watches and ends when nobody does.

use std::{collections::HashMap, sync::Arc, time::Duration};

use bytes::Bytes;
use futures_util::StreamExt as _;
use screen::link::{
    self, Access, Away, Display, FromHelper, Message, Notice, Source, Sources, ToHelper, VERSION,
};
use stream::{
    Region,
    viewer::{Input, KeyAction, MouseAction},
};
use stream_encode::{Encoder, Settings};
use tokio::{io::AsyncWriteExt as _, sync::mpsc, time::Instant};
use zbus::zvariant::OwnedObjectPath;

use super::{
    Arguments,
    away::Watch,
    capture::{Frames, Video, Wanted},
    clipboard::Clipboard,
    hand::Hand,
    input::Hands,
    notice::Shown,
    portal::{Desktop, Grant, Portal, Problem, Shared},
};

/// Messages on their way to the host: frames, the pointer and what the helper says.
pub(super) const MESSAGES: usize = 64;
/// How often the helper looks again whether it can share the screen while someone watches.
const LOOK_EVERY: Duration = Duration::from_secs(2);
/// How long the desktop may take to start a session that it granted before. Longer means that
/// it asks the person at the computer again, because the grant no longer holds.
const RESTORE_LIMIT: Duration = Duration::from_secs(20);
/// How long the person at the computer has to answer the desktop after turning sharing on.
const ASK_LIMIT: Duration = Duration::from_mins(5);
/// How long the screen may take to say that it locked after the desktop closed the session,
/// which GNOME does when it locks, rather than the person at the computer ending it.
const LOCKING: Duration = Duration::from_secs(1);
/// How long the desktop's services may take to answer the helper's first look, well within the
/// time that the host waits for the helper's first message.
const FIRST_LOOK: Duration = Duration::from_secs(3);
/// How long the desktop may take to show the pointer where the helper put it, before the helper
/// tells the host itself.
const POINTER_SHOWING: Duration = Duration::from_millis(100);

pub(super) enum Event {
    /// The helper reached the desktop's portal, and learned whether KDE lets Kodosi share the
    /// screen without asking, or could not reach it.
    Reached(Option<(Portal, bool)>),
    /// A portal session started, or did not; `asked` when the person at the computer turned the
    /// sharing on.
    Started {
        asked: bool,
        desktop: Result<Desktop, Problem>,
    },
    /// The desktop closed the portal session.
    Closed,
    /// A moment after the desktop closed the session, when the screen said whether it locked.
    Settled,
    /// The person at the computer stopped the sharing in the notice.
    Stopped,
    /// Text copied on the computer.
    Copied(String),
    /// The capture or its encoder failed.
    Failed(String),
    /// The lock or the account in front of the display changed.
    Changed,
    /// The pixels of a display's stream, which tell its scale.
    Sized { node: u32, width: u32, height: u32 },
}

/// Serves the host on standard input and output until it quits or goes away.
pub(super) async fn serve(arguments: Arguments) {
    let (say, saying) = mpsc::channel(MESSAGES);
    tokio::spawn(write(saying));
    let (events, hearing) = mpsc::unbounded_channel();
    let (reached, watch) = tokio::join!(
        tokio::time::timeout(FIRST_LOOK, reach()),
        tokio::time::timeout(FIRST_LOOK, Watch::start(events.clone())),
    );
    let (portal, preauthorized) = reached.ok().flatten().unzip();
    let mut session = Session {
        say,
        events,
        portal,
        reaching: false,
        preauthorized: preauthorized.unwrap_or_default(),
        grant: Grant::read(arguments.grant),
        access: Access::default(),
        away: None,
        watch: watch.ok(),
        watched: false,
        sharing: Sharing::Off,
        sources: Sources::default(),
        scales: HashMap::new(),
        capture: None,
        frames: None,
        notice: None,
        awake: None,
        hand: Arc::default(),
        pointed: None,
        copied: None,
    };
    session.hello(arguments.token).await;
    session.run(hearing).await;
}

async fn reach() -> Option<(Portal, bool)> {
    let portal = Portal::connect().await?;
    let preauthorized = portal.preauthorized().await;
    Some((portal, preauthorized))
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
    say: mpsc::Sender<Bytes>,
    events: mpsc::UnboundedSender<Event>,
    /// `None` until the helper reaches the desktop's portal.
    portal: Option<Portal>,
    /// The helper is on its way to the portal.
    reaching: bool,
    /// KDE lets Kodosi share the screen without asking.
    preauthorized: bool,
    grant: Grant,
    access: Access,
    away: Option<Away>,
    watch: Option<Watch>,
    watched: bool,
    sharing: Sharing,
    sources: Sources,
    /// Pixels per point of the displays whose streams told them, by where they are.
    scales: HashMap<Place, f64>,
    /// The captured display.
    capture: Option<u32>,
    /// The encoder, from the first capture until the host stops it.
    frames: Option<Arc<Frames>>,
    notice: Option<Shown>,
    /// The portal's request that keeps the screen awake while it is captured.
    awake: Option<OwnedObjectPath>,
    hand: Arc<Hand>,
    /// Where the helper last put the pointer, in the shown display's points, until the desktop
    /// shows it there or it is time to tell the host.
    pointed: Option<(Instant, (f64, f64))>,
    /// The text that last went between the computer and a viewer, which a viewer's device or a
    /// clipboard shared between devices brings back.
    copied: Option<String>,
}

/// Where the portal session is.
enum Sharing {
    Off,
    Starting,
    On(Linked),
    /// The desktop closed the session, and the helper waits to learn whether the screen locked.
    Settling,
}

/// A started portal session and what goes through it, which end in this order.
struct Linked {
    video: Video,
    hands: Option<Hands>,
    clipboard: Option<Clipboard>,
    closing: tokio::task::JoinHandle<()>,
    /// Closes the session once the rest is gone.
    desktop: Desktop,
}

impl Drop for Linked {
    fn drop(&mut self) {
        self.closing.abort();
    }
}

/// Where a display is and its size, in whole logical pixels.
type Place = (i64, i64, i64, i64);

impl Session {
    async fn hello(&mut self, token: String) {
        self.access = self.granted();
        self.away = self.watch.as_ref().and_then(Watch::away);
        self.tell(FromHelper::Hello {
            token,
            version: VERSION,
            encodable: Encoder::encodable(),
            largest: Encoder::largest(),
            access: self.access,
            away: self.away,
        })
        .await;
    }

    async fn run(&mut self, mut events: mpsc::UnboundedReceiver<Event>) {
        let (heard, mut hearing) = mpsc::channel(MESSAGES);
        tokio::spawn(read(heard));
        let mut looking = tokio::time::interval(LOOK_EVERY);
        looking.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let pointed = self.pointed.map(|(at, _)| at);
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
                () = tokio::time::sleep_until(pointed.unwrap_or_else(Instant::now)), if pointed.is_some() => {
                    self.point();
                }
            }
        }
        self.stop().await;
        self.sharing = Sharing::Off;
        self.notice = None;
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
                let settings = Settings { format, bitrate };
                if let Err(problem) = self.capture(source, region, width, height, settings).await {
                    self.fail(problem).await;
                }
            }
            ToHelper::Stop => self.stop().await,
            ToHelper::Keyframe => self.encoder(Encoder::want_key),
            ToHelper::Recover => self.encoder(Encoder::recover),
            ToHelper::Held { marks } => self.encoder(|encoder| encoder.held(&marks)),
            ToHelper::Bitrate { bitrate } => self.encoder(|encoder| encoder.set_bitrate(bitrate)),
            ToHelper::Input { input } => self.play(input),
            ToHelper::Notice { notice } => self.notice(notice).await,
            ToHelper::Clipboard { text } => {
                self.copied = Some(text.clone());
                if let Some(clipboard) = self.linked().and_then(|linked| linked.clipboard.as_ref())
                {
                    clipboard.put(text).await;
                }
            }
            ToHelper::Ask => self.ask().await,
            ToHelper::Quit => return false,
        }
        true
    }

    async fn capture(
        &mut self,
        source: Source,
        region: Option<Region>,
        width: u32,
        height: u32,
        settings: Settings,
    ) -> Result<(), String> {
        let Source::Display { id } = source else {
            return Err("Kodosi Screen shares displays on Linux, not windows".to_owned());
        };
        let Some(linked) = self.linked() else {
            return Err("the desktop does not share the screen yet".to_owned());
        };
        let shared = display(&linked.desktop, id).ok_or("the display is gone")?;
        let frames =
            Arc::clone(self.frames.get_or_insert_with(|| {
                Frames::new(settings, self.say.clone(), self.events.clone())
            }));
        frames.encode_as(settings);
        if let Some(linked) = self.linked() {
            linked.video.show(
                Wanted {
                    node: shared.node,
                    origin: (shared.x, shared.y),
                    logical: (shared.width, shared.height),
                    region,
                    width,
                    height,
                },
                frames,
            );
        }
        self.capture = Some(id);
        if self.awake.is_none()
            && let Some(portal) = &self.portal
        {
            self.awake = portal.inhibit().await;
        }
        Ok(())
    }

    async fn stop(&mut self) {
        if let Some(linked) = self.linked() {
            linked.video.hide();
        }
        self.capture = None;
        self.frames = None;
        if let (Some(portal), Some(awake)) = (&self.portal, self.awake.take()) {
            portal.uninhibit(&awake).await;
        }
    }

    /// Stops the capture after it failed, and the portal session with it; the host asks again,
    /// once the helper shared the screen anew.
    async fn fail(&mut self, problem: String) {
        self.stop().await;
        self.unshare().await;
        self.tell(FromHelper::Failed { problem }).await;
    }

    fn encoder(&self, work: impl FnOnce(&mut Encoder)) {
        if let Some(frames) = &self.frames {
            frames.encoder(work);
        }
    }

    fn play(&mut self, input: Input) {
        if self.hand.busy() && !lets_go(&input) {
            return;
        }
        let (Some(id), Some(linked)) = (self.capture, self.linked()) else {
            return;
        };
        let (Some(hands), Some(shared), true) = (
            &linked.hands,
            display(&linked.desktop, id),
            self.access.input,
        ) else {
            return;
        };
        let pointed = match input {
            Input::Mouse { x, y, .. } => Some((x, y)),
            Input::Key { .. } | Input::Text { .. } => None,
        };
        hands.play(input, (shared.x, shared.y));
        if let Some(at) = pointed {
            self.pointed = Some((Instant::now() + POINTER_SHOWING, at));
        }
    }

    /// Tells the host where the helper put the pointer, when the desktop did not show it there.
    fn point(&mut self) {
        let Some((_, at)) = self.pointed.take() else {
            return;
        };
        let (Some(id), Some(linked)) = (self.capture, self.linked()) else {
            return;
        };
        if let Some(shared) = display(&linked.desktop, id)
            && !self.hand.showed((shared.x + at.0, shared.y + at.1))
        {
            linked.video.point(at);
        }
    }

    async fn notice(&mut self, notice: Option<Notice>) {
        let first = notice.is_some() && !self.watched;
        self.watched = notice.is_some();
        match notice {
            Some(notice) => match &self.notice {
                Some(shown) => shown.update(&notice).await,
                None => self.notice = Shown::show(&notice, self.events.clone()).await,
            },
            None => self.notice = None,
        }
        if !self.watched {
            self.stop().await;
            self.unshare().await;
            return;
        }
        // Opening the screen wakes its display.
        if first && let Some(watch) = &self.watch {
            watch.wake().await;
        }
        self.share();
    }

    /// The person at the computer turned the sharing on, so it is the time for the desktop to
    /// ask them, unless KDE takes their word for it.
    async fn ask(&mut self) {
        let Some(portal) = self.portal.clone() else {
            return;
        };
        if self.access.screen && self.access.input {
            return;
        }
        if portal.preauthorize().await {
            self.preauthorized = true;
            self.allow(self.granted()).await;
            self.share();
            return;
        }
        if !matches!(self.sharing, Sharing::Off) {
            return;
        }
        self.sharing = Sharing::Starting;
        let events = self.events.clone();
        tokio::spawn(async move {
            let desktop = portal.start(None, ASK_LIMIT).await;
            drop(events.send(Event::Started {
                asked: true,
                desktop,
            }));
        });
    }

    /// Starts a portal session when someone watches a screen that shows and that the desktop
    /// lets the helper share without asking.
    fn share(&mut self) {
        if !self.shows() || !self.access.screen || !matches!(self.sharing, Sharing::Off) {
            return;
        }
        let Some(portal) = self.portal.clone() else {
            return;
        };
        self.sharing = Sharing::Starting;
        let restore = self.grant.token();
        let events = self.events.clone();
        tokio::spawn(async move {
            let desktop = portal.start(restore, RESTORE_LIMIT).await;
            drop(events.send(Event::Started {
                asked: false,
                desktop,
            }));
        });
    }

    const fn shows(&self) -> bool {
        self.watched && self.away.is_none()
    }

    fn linked(&self) -> Option<&Linked> {
        match &self.sharing {
            Sharing::On(linked) => Some(linked),
            Sharing::Off | Sharing::Starting | Sharing::Settling => None,
        }
    }

    /// Ends the portal session, if any.
    async fn unshare(&mut self) {
        self.capture = None;
        self.frames = None;
        if matches!(self.sharing, Sharing::On(_)) {
            self.sharing = Sharing::Off;
        }
        self.allow(self.granted()).await;
    }

    async fn started(&mut self, asked: bool, desktop: Result<Desktop, Problem>) {
        if matches!(self.sharing, Sharing::Starting) {
            self.sharing = Sharing::Off;
        }
        match desktop {
            Ok(desktop) => {
                self.grant.keep(desktop.token.clone(), desktop.input).await;
                if self.shows() && matches!(self.sharing, Sharing::Off) {
                    self.link(desktop).await;
                }
            }
            Err(Problem::Declined) => {
                if !asked {
                    self.grant.forget().await;
                }
                if let Some(portal) = &self.portal {
                    self.preauthorized = portal.preauthorized().await;
                }
            }
            Err(Problem::Failed(problem)) => {
                if self.watched {
                    self.tell(FromHelper::Failed { problem }).await;
                }
            }
        }
        self.allow(self.granted()).await;
    }

    /// Takes the session's streams, input and clipboard, and tells the host which displays it
    /// shares, so that it asks for a capture.
    async fn link(&mut self, desktop: Desktop) {
        let Some(video) = desktop.pipewire().await.ok().and_then(|remote| {
            Video::open(
                remote,
                self.say.clone(),
                self.events.clone(),
                Arc::clone(&self.hand),
            )
        }) else {
            return;
        };
        let Ok(mut closed) = desktop.closing().await else {
            return;
        };
        let hands = if desktop.input {
            let hand = Arc::clone(&self.hand);
            desktop.eis().await.ok().map(|eis| Hands::start(eis, hand))
        } else {
            None
        };
        let clipboard = if desktop.clipboard {
            Clipboard::start(&desktop, self.events.clone()).await
        } else {
            None
        };
        let events = self.events.clone();
        let closing = tokio::spawn(async move {
            if closed.next().await.is_some() {
                drop(events.send(Event::Closed));
            }
        });
        self.sharing = Sharing::On(Linked {
            video,
            hands,
            clipboard,
            closing,
            desktop,
        });
        self.tell_sources().await;
    }

    async fn event(&mut self, event: Event) {
        match event {
            Event::Reached(reached) => {
                self.reaching = false;
                if let Some((portal, preauthorized)) = reached {
                    self.portal = Some(portal);
                    self.preauthorized = preauthorized;
                    self.allow(self.granted()).await;
                    self.share();
                }
            }
            Event::Started { asked, desktop } => self.started(asked, desktop).await,
            Event::Closed => {
                if self.linked().is_some() {
                    self.stop().await;
                    self.unshare().await;
                    self.sharing = Sharing::Settling;
                    let events = self.events.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(LOCKING).await;
                        drop(events.send(Event::Settled));
                    });
                }
            }
            Event::Settled => {
                if matches!(self.sharing, Sharing::Settling) {
                    self.sharing = Sharing::Off;
                }
                self.look_away().await;
                if self.away.is_none() {
                    self.stopped().await;
                }
            }
            Event::Stopped => self.stopped().await,
            Event::Copied(text) => {
                if self.copied.as_ref() != Some(&text) {
                    self.copied = Some(text.clone());
                    self.tell(FromHelper::Copied { text }).await;
                }
            }
            Event::Failed(problem) => {
                if self.linked().is_some() {
                    self.fail(problem).await;
                }
            }
            Event::Changed => self.look().await,
            Event::Sized {
                node,
                width,
                height,
            } => self.sized(node, width, height).await,
        }
    }

    /// The person at the computer stopped the sharing there, so the host turns it off.
    async fn stopped(&mut self) {
        self.stop().await;
        self.unshare().await;
        self.notice = None;
        self.tell(FromHelper::Stopped).await;
    }

    async fn look(&mut self) {
        if self.portal.is_none() && !self.reaching {
            self.reaching = true;
            let events = self.events.clone();
            tokio::spawn(async move { drop(events.send(Event::Reached(reach().await))) });
        }
        self.look_away().await;
        if self.away.is_some() {
            if self.linked().is_some() {
                self.stop().await;
                self.unshare().await;
            }
        } else {
            self.share();
        }
    }

    async fn look_away(&mut self) {
        let away = self.watch.as_ref().and_then(Watch::away);
        if away != self.away {
            self.away = away;
            self.tell(FromHelper::Away { away }).await;
        }
    }

    /// A stream tells how many pixels its display has, so its points have their true scale.
    async fn sized(&mut self, node: u32, width: u32, height: u32) {
        let Some(shared) = self.linked().and_then(|linked| {
            linked
                .desktop
                .displays
                .iter()
                .find(|shared| shared.node == node)
                .copied()
        }) else {
            return;
        };
        if shared.width <= 0.0 || shared.height <= 0.0 {
            return;
        }
        let scale = (f64::from(width) / shared.width).max(f64::from(height) / shared.height);
        self.scales.insert(place(&shared), scale);
        self.tell_sources().await;
    }

    async fn tell_sources(&mut self) {
        let Some(linked) = self.linked() else {
            return;
        };
        let displays = (1..)
            .zip(&linked.desktop.displays)
            .map(|(id, shared)| Display {
                id,
                width: shared.width,
                height: shared.height,
                scale: self.scales.get(&place(shared)).copied().unwrap_or(1.0),
            })
            .collect();
        let sources = Sources {
            displays,
            windows: Vec::new(),
        };
        if sources != self.sources {
            self.sources = sources.clone();
            self.tell(FromHelper::Sources { sources }).await;
        }
    }

    /// What the desktop lets the helper do without asking anyone: what KDE pre-authorized, what
    /// the desktop granted before, or the session that it shares now.
    fn granted(&self) -> Access {
        let kept = self.grant.token().is_some();
        let linked = self.linked().map(|linked| linked.desktop.input);
        Access {
            screen: self.preauthorized || kept || linked.is_some(),
            input: self.preauthorized || (kept && self.grant.input()) || linked == Some(true),
        }
    }

    async fn allow(&mut self, access: Access) {
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

/// Displays have the ids 1, 2, … in the portal's order.
fn display(desktop: &Desktop, id: u32) -> Option<Shared> {
    let index = usize::try_from(id.checked_sub(1)?).ok()?;
    desktop.displays.get(index).copied()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "a desktop's logical pixels fit an i64"
)]
fn place(shared: &Shared) -> Place {
    (
        shared.x.round() as i64,
        shared.y.round() as i64,
        shared.width.round() as i64,
        shared.height.round() as i64,
    )
}
