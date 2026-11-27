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
    input::{Hands, Placed},
    notice::Tray,
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
    /// The encoder needs the newest picture again.
    Redraw,
    /// The lock or the account in front of the display changed.
    Changed,
    /// The pixels of a display's stream, first and after each change of the display.
    Sized { node: u32, pixels: (u32, u32) },
    /// Where the desktop's libei server places the displays, and their scales, first and after
    /// each change of them.
    Placed(Vec<Placed>),
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
    let tray = Tray::start(events.clone());
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
        streamed: HashMap::new(),
        placed: Vec::new(),
        places: HashMap::new(),
        capture: None,
        frames: None,
        tray,
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
    /// Pixels per point of the displays whose streams told them, by where the portal placed
    /// them, for the next portal session.
    scales: HashMap<Place, f64>,
    /// What the streams of this portal session told of their displays, by node.
    streamed: HashMap<u32, Streamed>,
    /// Where the desktop's libei server places the displays now, and their scales.
    placed: Vec<Placed>,
    /// The index in `placed` of each display of this portal session, by its node.
    places: HashMap<u32, usize>,
    /// The captured display.
    capture: Option<u32>,
    /// The encoder, from the first capture until the host stops it.
    frames: Option<Arc<Frames>>,
    tray: Tray,
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
        self.tray.show(None);
    }

    /// Returns false when the host wants the helper to quit.
    async fn hear(&mut self, said: ToHelper) -> bool {
        match said {
            ToHelper::Capture {
                source,
                size,
                region,
                width,
                height,
                format,
                bitrate,
                pace,
            } => {
                let settings = Settings {
                    format,
                    bitrate,
                    pace,
                };
                let shown = self.capture(source, size, region, (width, height), settings);
                if let Err(problem) = shown.await {
                    self.fail(problem).await;
                }
            }
            ToHelper::Stop => self.stop().await,
            ToHelper::Keyframe => self.encoder(Encoder::want_key),
            ToHelper::Recover => self.encoder(Encoder::recover),
            ToHelper::Held { marks } => self.encoder(|encoder| encoder.held(&marks)),
            ToHelper::Bitrate { bitrate } => self.encoder(|encoder| encoder.set_bitrate(bitrate)),
            ToHelper::Pace { pace } => self.encoder(|encoder| encoder.set_pace(pace)),
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
            // The desktop's portal asks the person at the computer before it shares the screen.
            ToHelper::Owner { .. } => self.tell(FromHelper::Owner { allowed: true }).await,
            ToHelper::Allow { .. } => {
                self.tell(FromHelper::Allowed {
                    allowed: true,
                    problem: None,
                })
                .await;
            }
            ToHelper::Forget => {}
            ToHelper::Quit => return false,
        }
        true
    }

    /// Captures `region` of a display whose size in points is `size`, scaled to `pixels`. The
    /// host asked before it heard that the display took another size when `size` is not the
    /// display's: it asks again for the new size.
    async fn capture(
        &mut self,
        source: Source,
        size: (f64, f64),
        region: Option<Region>,
        pixels: (u32, u32),
        settings: Settings,
    ) -> Result<(), String> {
        let Source::Display { id } = source else {
            return Err("Kodosi Screen shares displays on Linux, not windows".to_owned());
        };
        let Some(linked) = self.linked() else {
            return Err("the desktop does not share the screen yet".to_owned());
        };
        let shared = display(&linked.desktop, id).ok_or("the display is gone")?;
        let known = self.known(&shared);
        if !same_size(known.points, size) {
            return Ok(());
        }
        let frames =
            Arc::clone(self.frames.get_or_insert_with(|| {
                Frames::new(settings, self.say.clone(), self.events.clone())
            }));
        frames.encode_as(settings);
        if let Some(linked) = self.linked() {
            linked.video.show(
                Wanted {
                    node: shared.node,
                    origin: known.origin,
                    logical: known.points,
                    stream: known.pixels,
                    region,
                    width: pixels.0,
                    height: pixels.1,
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
        hands.play(input, self.known(&shared).origin);
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
        if let Some(shared) = display(&linked.desktop, id) {
            let origin = self.known(&shared).origin;
            if !self.hand.showed((origin.0 + at.0, origin.1 + at.1)) {
                linked.video.point(at);
            }
        }
    }

    async fn notice(&mut self, notice: Option<Notice>) {
        let first = notice.is_some() && !self.watched;
        self.watched = notice.is_some();
        self.tray.show(notice);
        if !self.watched {
            self.stop().await;
            self.unshare().await;
            return;
        }
        // Opening the screen wakes its display.
        if first && let Some(watch) = &self.watch {
            watch.wake();
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
            let events = self.events.clone();
            desktop
                .eis()
                .await
                .ok()
                .map(|eis| Hands::start(eis, hand, events))
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
        self.streamed.clear();
        self.placed.clear();
        self.places.clear();
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
            Event::Redraw => {
                if let Some(linked) = self.linked() {
                    linked.video.redraw();
                }
            }
            Event::Changed => self.look().await,
            Event::Sized { node, pixels } => self.sized(node, pixels).await,
            Event::Placed(placed) => self.placed(placed).await,
        }
    }

    /// The person at the computer stopped the sharing there, so the host turns it off.
    async fn stopped(&mut self) {
        self.stop().await;
        self.unshare().await;
        self.tray.show(None);
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

    /// A stream tells how many pixels its display has, first and after each change of the
    /// display: its points follow at the display's scale, which the first size tells.
    async fn sized(&mut self, node: u32, pixels: (u32, u32)) {
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
        if pixels.0 == 0 || pixels.1 == 0 || shared.width <= 0.0 || shared.height <= 0.0 {
            return;
        }
        let scale = if let Some(streamed) = self.streamed.get(&node) {
            streamed.scale
        } else {
            let scale = scale((shared.width, shared.height), pixels);
            self.scales.insert(place(&shared), scale);
            scale
        };
        self.streamed.insert(node, Streamed { pixels, scale });
        self.tell_sources().await;
    }

    /// The desktop's libei server tells where the displays are and their scales, first and after
    /// each change of them. A display keeps its place in the server's list while no display comes
    /// or goes; at first, its place is where the portal put it.
    async fn placed(&mut self, placed: Vec<Placed>) {
        if placed.len() != self.placed.len() {
            self.places.clear();
        }
        if let Some(linked) = self.linked() {
            let found: Vec<(u32, usize)> = linked
                .desktop
                .displays
                .iter()
                .filter(|shared| !self.places.contains_key(&shared.node))
                .filter_map(|shared| {
                    let known = self.known(shared);
                    let index = placed.iter().position(|place| holds(place, &known))?;
                    Some((shared.node, index))
                })
                .collect();
            self.places.extend(found);
        }
        self.placed = placed;
        self.tell_sources().await;
    }

    /// A display as the portal told it when its session started, with what its stream and the
    /// desktop's libei server told since.
    fn known(&self, shared: &Shared) -> Known {
        let placed = self
            .places
            .get(&shared.node)
            .and_then(|index| self.placed.get(*index))
            .filter(|placed| placed.scale > 0.0);
        let streamed = self.streamed.get(&shared.node);
        let scale = placed
            .map(|placed| placed.scale)
            .or_else(|| streamed.map(|streamed| streamed.scale))
            .or_else(|| self.scales.get(&place(shared)).copied())
            .unwrap_or(1.0);
        let points = match (streamed, placed) {
            (Some(streamed), _) => points(streamed.pixels, scale),
            (None, Some(placed)) => (placed.width, placed.height),
            (None, None) => (shared.width, shared.height),
        };
        Known {
            origin: placed.map_or((shared.x, shared.y), |placed| (placed.x, placed.y)),
            points,
            scale,
            pixels: streamed.map(|streamed| streamed.pixels),
        }
    }

    async fn tell_sources(&mut self) {
        let Some(linked) = self.linked() else {
            return;
        };
        let displays = (1..)
            .zip(&linked.desktop.displays)
            .map(|(id, shared)| {
                let known = self.known(shared);
                Display {
                    id,
                    width: known.points.0,
                    height: known.points.1,
                    scale: known.scale,
                }
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

/// What a display's stream told: its pixels, now, and its pixels per point, from its first size.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Streamed {
    pixels: (u32, u32),
    scale: f64,
}

/// A shared display as the helper knows it now.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Known {
    /// Where its top left is on the desktop, in points.
    origin: (f64, f64),
    /// Its size in points.
    points: (f64, f64),
    /// Its pixels per point.
    scale: f64,
    /// Its stream's size in pixels, once the stream told it.
    pixels: Option<(u32, u32)>,
}

/// A display's points follow its stream's pixels: the portal tells them only when its session
/// starts, and its stream tells when the display turns or takes another resolution.
fn points(pixels: (u32, u32), scale: f64) -> (f64, f64) {
    (f64::from(pixels.0) / scale, f64::from(pixels.1) / scale)
}

/// Whether the desktop's libei server places a display where the helper knows it, in whole
/// logical pixels.
fn holds(placed: &Placed, known: &Known) -> bool {
    let near = |one: f64, other: f64| (one - other).abs() < 1.0;
    near(placed.x, known.origin.0)
        && near(placed.y, known.origin.1)
        && near(placed.width, known.points.0)
        && near(placed.height, known.points.1)
}

/// Pixels per point of a display of `points` as the portal told them, whose stream has
/// `pixels`. A display that turned since the portal told its points has them the other way.
fn scale(points: (f64, f64), pixels: (u32, u32)) -> f64 {
    let (width, height) = (f64::from(pixels.0), f64::from(pixels.1));
    let (across, down) = if (points.0 >= points.1) == (width >= height) {
        points
    } else {
        (points.1, points.0)
    };
    (width / across).max(height / down)
}

/// The host's sizes come back from its JSON, which may round a point's last bit.
fn same_size(one: (f64, f64), other: (f64, f64)) -> bool {
    const ROUNDING: f64 = 0.001;
    (one.0 - other.0).abs() < ROUNDING && (one.1 - other.1).abs() < ROUNDING
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_display_that_turns_or_takes_another_resolution_keeps_its_scale_and_its_shape() {
        let wide = (2560, 1440);
        let scale = scale((1707.0, 960.0), wide);
        assert!((scale - 1.5).abs() < 1e-9, "{scale}");
        assert!(same_size(
            points((1440, 2560), scale),
            (960.0, 2560.0 / 1.5)
        ));
        assert!(same_size(points((1920, 1200), scale), (1280.0, 800.0)));
        assert!((self::scale((1920.0, 1080.0), (1080, 1920)) - 1.0).abs() < 1e-9);
    }
}
