//! One helper's capture and the viewers that share it.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, PoisonError},
    time::{Duration, Instant},
};

use bytes::Bytes;
use stream::{
    Ask, Codec, Decoder, Encoded, Flow, Format, Region, Size, Viewing,
    viewer::{Frame, Input, KeyAction, MouseAction, MouseButton, Viewer},
};
use tokio::{
    io::AsyncWriteExt as _,
    sync::{Notify, mpsc, oneshot},
};

use crate::{
    host::Started,
    link::{self, Access, FromHelper, Message, Notice, Pointer, Shape, Source, Sources, ToHelper},
    wire::{State, ToScreen},
};

const TICK: Duration = Duration::from_millis(100);
const HELPER_MESSAGES: usize = 256;
const RETRY_CAPTURE: Duration = Duration::from_secs(1);
/// How long the helper stays after the last viewer left, for a quick return.
const IDLE: Duration = Duration::from_mins(1);
/// The largest picture `VideoToolbox` encodes in real time (long side, short side).
const MOST_PICTURE: (f64, f64) = (4096.0, 2304.0);
/// The smallest region a viewer zooms into, in points.
const SMALLEST_REGION: f64 = 32.0;

pub(crate) enum Event {
    Join {
        viewer: Viewer,
        seat: u64,
        outgoing: mpsc::Sender<Bytes>,
        mailbox: Arc<Mailbox>,
    },
    Message {
        viewer: String,
        message: ToScreen,
    },
    Leave {
        viewer: String,
        seat: u64,
    },
    Show(Wanted),
    Access(oneshot::Sender<Access>),
    Ask,
    Stop,
}

/// What an agent showed: the main display, or the window whose app or title holds the words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wanted {
    Screen,
    Window(String),
}

/// The newest state for one viewer. Replacing an unread state keeps what it carried once.
#[derive(Default)]
pub(crate) struct Mailbox {
    state: std::sync::Mutex<Option<State>>,
    pub(crate) ready: Notify,
}

impl Mailbox {
    fn put(&self, state: State) {
        let mut waiting = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let state = match waiting.take() {
            Some(earlier) => state.after(earlier),
            None => state,
        };
        *waiting = Some(state);
        drop(waiting);
        self.ready.notify_one();
    }

    pub(crate) fn take(&self) -> Option<State> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

struct Seat {
    viewer: Viewer,
    number: u64,
    outgoing: mpsc::Sender<Bytes>,
    mailbox: Arc<Mailbox>,
    size: Option<Size>,
    decoders: Vec<Decoder>,
    anchors: bool,
    watching: bool,
    /// The part of the source that the view shows, when only a part.
    look: Option<Region>,
    has_sources: bool,
    shapes: HashSet<u64>,
    pressed: Pressed,
}

/// The button and keys that a viewer holds down on the computer, which the computer would go on
/// holding if the viewer went away before letting go.
#[derive(Default)]
struct Pressed {
    button: MouseButton,
    clicks: u8,
    at: (f64, f64),
    /// `key`, `code` and `key_code` of each key down, in the order pressed.
    keys: Vec<(String, String, u32)>,
}

impl Pressed {
    fn note(&mut self, input: &Input) {
        match input {
            Input::Mouse {
                action,
                x,
                y,
                button,
                clicks,
                ..
            } => {
                self.at = (*x, *y);
                match action {
                    MouseAction::Press => (self.button, self.clicks) = (*button, *clicks),
                    MouseAction::Release => self.button = MouseButton::None,
                    MouseAction::Move | MouseAction::Wheel => {}
                }
            }
            Input::Key {
                action,
                key,
                code,
                key_code,
                ..
            } => {
                self.keys.retain(|(_, held, _)| held != code);
                if *action == KeyAction::Down {
                    self.keys.push((key.clone(), code.clone(), *key_code));
                }
            }
            Input::Text { .. } => {}
        }
    }

    /// The input that lets go of the button, then of the keys, the last pressed first.
    fn released(self) -> impl Iterator<Item = Input> {
        let (x, y) = self.at;
        let button = (self.button != MouseButton::None).then_some(Input::Mouse {
            action: MouseAction::Release,
            x,
            y,
            button: self.button,
            clicks: self.clicks,
            delta_x: 0.0,
            delta_y: 0.0,
            modifiers: 0,
        });
        let keys = self
            .keys
            .into_iter()
            .rev()
            .map(|(key, code, key_code)| Input::Key {
                action: KeyAction::Up,
                key,
                code,
                text: None,
                key_code,
                modifiers: 0,
            });
        button.into_iter().chain(keys)
    }
}

impl Seat {
    fn viewing(&self) -> Option<Viewing<'_>> {
        self.watching.then_some(Viewing {
            decoders: &self.decoders,
            anchors: self.anchors,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Capture {
    source: Source,
    region: Option<Region>,
    width: u32,
    height: u32,
    format: Format,
}

/// What the host gives a screen to tell the computer's owner about it.
pub(crate) struct Calls {
    pub(crate) notice: Arc<dyn Fn(&[String]) -> Notice + Send + Sync>,
    pub(crate) stopped: Arc<dyn Fn() + Send + Sync>,
}

pub(crate) struct Live {
    helper: mpsc::Sender<Bytes>,
    encodable: Vec<Codec>,
    access: Access,
    calls: Calls,
    flow: Flow,
    seats: HashMap<String, Seat>,
    sources: Sources,
    source: Option<Source>,
    wanted: Wanted,
    pointer: Option<Pointer>,
    shapes: HashMap<u64, Shape>,
    /// The viewer whose view the capture's size follows: the last to act, or the first to come.
    control: Option<String>,
    capture: Option<Capture>,
    capture_failed_at: Option<Instant>,
    next_frame: u32,
    dirty: bool,
    idle_since: Option<Instant>,
}

impl Live {
    pub(crate) fn new(
        started: Started,
        wanted: Wanted,
        calls: Calls,
    ) -> (Self, tokio::net::unix::OwnedReadHalf) {
        let (helper, mut writing) = mpsc::channel::<Bytes>(HELPER_MESSAGES);
        let mut writer = started.writer;
        tokio::spawn(async move {
            while let Some(message) = writing.recv().await {
                if writer.write_all(&message).await.is_err() {
                    return;
                }
            }
        });
        let live = Self {
            helper,
            encodable: started.encodable,
            access: started.access,
            calls,
            flow: Flow::new(),
            seats: HashMap::new(),
            sources: Sources::default(),
            source: None,
            wanted,
            pointer: None,
            shapes: HashMap::new(),
            control: None,
            capture: None,
            capture_failed_at: None,
            next_frame: 0,
            dirty: false,
            idle_since: Some(Instant::now()),
        };
        (live, started.reader)
    }

    pub(crate) async fn run(
        mut self,
        mut reader: tokio::net::unix::OwnedReadHalf,
        mut events: mpsc::Receiver<Event>,
    ) {
        let (heard, mut hearing) = mpsc::channel(HELPER_MESSAGES);
        tokio::spawn(async move {
            while let Ok(Some(message)) = link::read::<FromHelper>(&mut reader).await {
                if heard.send(message).await.is_err() {
                    return;
                }
            }
        });
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let going = tokio::select! {
                event = events.recv() => event.is_some_and(|event| self.handle(event)),
                message = hearing.recv() => message.is_some_and(|message| self.hear(message)),
                _ = tick.tick() => self.tick(),
            };
            if !going {
                break;
            }
        }
        for seat in std::mem::take(&mut self.seats).into_values() {
            self.let_go(seat.pressed);
        }
        self.say(&ToHelper::Quit);
    }

    /// Returns false when the screen ends.
    fn handle(&mut self, event: Event) -> bool {
        match event {
            Event::Join {
                viewer,
                seat,
                outgoing,
                mailbox,
            } => {
                let replaced = self.seats.insert(
                    viewer.id.clone(),
                    Seat {
                        viewer,
                        number: seat,
                        outgoing,
                        mailbox,
                        size: None,
                        decoders: Vec::new(),
                        anchors: false,
                        watching: false,
                        look: None,
                        has_sources: false,
                        shapes: HashSet::new(),
                        pressed: Pressed::default(),
                    },
                );
                if let Some(replaced) = replaced {
                    self.let_go(replaced.pressed);
                }
            }
            Event::Message { viewer, message } => self.message(&viewer, message),
            Event::Leave { viewer, seat } => {
                if self
                    .seats
                    .get(&viewer)
                    .is_some_and(|left| left.number == seat)
                    && let Some(left) = self.seats.remove(&viewer)
                {
                    self.let_go(left.pressed);
                    self.flow.leave(&viewer);
                    self.changed_watchers();
                }
            }
            Event::Show(wanted) => {
                self.wanted = wanted;
                self.show(self.pick());
            }
            Event::Access(reply) => drop(reply.send(self.access)),
            Event::Ask => self.say(&ToHelper::Ask),
            Event::Stop => return false,
        }
        true
    }

    fn message(&mut self, viewer: &str, message: ToScreen) {
        let now = Instant::now();
        match message {
            ToScreen::Watch {
                size,
                decoders,
                anchors,
                opened,
            } => {
                let Some(seat) = self.seats.get_mut(viewer) else {
                    return;
                };
                seat.size = Some(size);
                seat.decoders = decoders;
                seat.anchors = anchors;
                seat.watching = true;
                seat.has_sources = false;
                seat.shapes.clear();
                let local = seat.viewer.local;
                self.flow.seat(viewer, anchors, local, now);
                if opened || self.control.is_none() {
                    self.control = Some(viewer.to_owned());
                }
                self.changed_watchers();
                self.flow.keyframe_asked(now);
                self.say(&ToHelper::Keyframe);
            }
            ToScreen::Ack { frame } => self.flow.ack(viewer, frame, now),
            ToScreen::Rate { bitrate } => self.flow.rate(viewer, bitrate),
            ToScreen::Keyframe => {
                let seats = &self.seats;
                if let Some(ask) = self.flow.reset(viewer, now, |viewer| queued(seats, viewer)) {
                    self.ask(ask);
                }
            }
            ToScreen::Size { size } => {
                if let Some(seat) = self.seats.get_mut(viewer) {
                    seat.size = Some(size);
                    self.update_capture();
                }
            }
            ToScreen::Input { input } => {
                // Hovering, letting go and lifting a key follow what the viewer did before, so
                // only acting takes the capture over, as in the shared browser.
                let acts = !matches!(
                    input,
                    Input::Mouse {
                        action: MouseAction::Move | MouseAction::Release,
                        ..
                    } | Input::Key {
                        action: KeyAction::Up,
                        ..
                    }
                );
                if acts {
                    self.take_control(viewer);
                }
                if let Some(seat) = self.seats.get_mut(viewer) {
                    seat.pressed.note(&input);
                }
                self.say(&ToHelper::Input { input });
            }
            ToScreen::Show { source } => {
                if self.sources.size(source).is_some() {
                    self.show(Some(source));
                }
            }
            ToScreen::Look { source, region } => {
                if self.source != Some(source) {
                    return;
                }
                let look = region.and_then(|region| self.within(region));
                if let Some(seat) = self.seats.get_mut(viewer) {
                    seat.look = look;
                }
                self.take_control(viewer);
                self.update_capture();
            }
        }
    }

    /// Looks were parts of the source that was shown, so each view starts whole on another one.
    fn show(&mut self, source: Option<Source>) {
        if source != self.source {
            self.source = source;
            for seat in self.seats.values_mut() {
                seat.look = None;
            }
        }
        self.update_capture();
        self.dirty = true;
    }

    fn let_go(&self, pressed: Pressed) {
        for input in pressed.released() {
            self.say(&ToHelper::Input { input });
        }
    }

    fn take_control(&mut self, viewer: &str) {
        if self.control.as_deref() != Some(viewer) && self.seats.contains_key(viewer) {
            self.control = Some(viewer.to_owned());
            self.update_capture();
            self.dirty = true;
        }
    }

    /// Returns false when the screen ends.
    fn hear(&mut self, message: Message<FromHelper>) -> bool {
        match message {
            Message::Frame(picture) => self.frame(picture),
            Message::Said(said) => match said {
                FromHelper::Hello { .. } => {}
                FromHelper::Sources { sources } => {
                    self.sources = sources;
                    for seat in self.seats.values_mut() {
                        seat.has_sources = false;
                    }
                    let gone = self
                        .source
                        .is_none_or(|source| self.sources.size(source).is_none());
                    self.show(if gone { self.pick() } else { self.source });
                }
                FromHelper::Access { access } => {
                    self.access = access;
                    self.dirty = true;
                }
                FromHelper::Pointer { pointer } => {
                    self.pointer = pointer;
                    self.publish();
                }
                FromHelper::Shape { shape } => {
                    self.shapes.insert(shape.id, shape);
                }
                FromHelper::Stopped => {
                    (self.calls.stopped)();
                    return false;
                }
                FromHelper::Failed { .. } => {
                    self.capture = None;
                    self.capture_failed_at = Some(Instant::now());
                }
            },
        }
        true
    }

    /// Returns false when the screen ends.
    fn tick(&mut self) -> bool {
        let now = Instant::now();
        if self
            .capture_failed_at
            .is_some_and(|at| now.duration_since(at) >= RETRY_CAPTURE)
        {
            self.capture_failed_at = None;
            self.update_capture();
        }
        if let Some(bitrate) = self.flow.steer(self.control.as_deref(), now) {
            self.say(&ToHelper::Bitrate { bitrate });
        }
        let hopeless = self.flow.hopeless(now);
        if !hopeless.is_empty() {
            for viewer in hopeless {
                self.seats.remove(&viewer);
            }
            self.changed_watchers();
        }
        let seats = &self.seats;
        if self.capture.is_some()
            && let Some(ask) = self.flow.ask(now, |viewer| queued(seats, viewer))
        {
            self.ask(ask);
        }
        let marks = self.flow.held();
        if !marks.is_empty() {
            self.say(&ToHelper::Held { marks });
        }
        if self.dirty {
            self.publish();
        }
        if self.flow.is_empty() {
            let since = *self.idle_since.get_or_insert(now);
            return now.duration_since(since) < IDLE;
        }
        self.idle_since = None;
        true
    }

    fn ask(&self, ask: Ask) {
        self.say(&match ask {
            Ask::Keyframe => ToHelper::Keyframe,
            Ask::Recover => ToHelper::Recover,
        });
    }

    fn say(&self, message: &ToHelper) {
        drop(self.helper.try_send(link::json(message)));
    }

    fn frame(&mut self, picture: Encoded) {
        let now = Instant::now();
        self.next_frame = self.next_frame.wrapping_add(1);
        let id = self.next_frame;
        let seats = &self.seats;
        let delivery = self.flow.frame(
            id,
            picture.kind,
            picture.mark,
            picture.data.len(),
            now,
            |viewer| queued(seats, viewer),
        );
        if !delivery.to.is_empty() {
            let codec = picture.codec;
            let message = Frame {
                id,
                codec,
                kind: picture.kind,
                region: picture.region,
                data: picture.data,
            }
            .encode();
            for viewer in &delivery.to {
                let sent = self.seats.get(viewer).is_some_and(|seat| {
                    codec.decoded_by(&seat.decoders)
                        && seat.outgoing.try_send(message.clone()).is_ok()
                });
                if !sent {
                    self.flow.lost(viewer, id, now);
                }
            }
        }
        if let Some(ask) = delivery.ask {
            self.ask(ask);
        }
    }

    fn changed_watchers(&mut self) {
        if self
            .control
            .as_ref()
            .is_none_or(|control| !self.seats.get(control).is_some_and(|seat| seat.watching))
        {
            self.control = self
                .seats
                .iter()
                .find(|(_, seat)| seat.watching)
                .map(|(viewer, _)| viewer.clone());
        }
        let mut names: Vec<String> = self
            .seats
            .values()
            .filter(|seat| seat.watching)
            .map(|seat| seat.viewer.name.clone())
            .collect();
        names.sort();
        names.dedup();
        let notice = (!names.is_empty()).then(|| (self.calls.notice)(&names));
        self.say(&ToHelper::Notice { notice });
        self.update_capture();
        self.dirty = true;
    }

    fn pick(&self) -> Option<Source> {
        let Wanted::Window(words) = &self.wanted else {
            return self.sources.main();
        };
        let words = words.to_lowercase();
        self.sources
            .windows
            .iter()
            .find(|window| {
                window.app.to_lowercase().contains(&words)
                    || window.title.to_lowercase().contains(&words)
            })
            .map(|window| Source::Window { id: window.id })
            .or_else(|| self.sources.main())
    }

    /// Keeps a region inside the shown source and no smaller than a viewer zooms.
    fn within(&self, region: Region) -> Option<Region> {
        let (width, height, _) = self.sources.size(self.source?)?;
        let fit = |offset: f64, length: f64, whole: f64| {
            let length = length.clamp(SMALLEST_REGION.min(whole), whole);
            (offset.clamp(0.0, whole - length), length)
        };
        let (x, region_width) = fit(region.x, region.width, width);
        let (y, region_height) = fit(region.y, region.height, height);
        let whole = region_width >= width && region_height >= height;
        (!whole).then_some(Region {
            x,
            y,
            width: region_width,
            height: region_height,
        })
    }

    fn update_capture(&mut self) {
        let wanted = self.wanted_capture();
        if wanted == self.capture {
            return;
        }
        self.capture = wanted;
        match wanted {
            Some(capture) => {
                self.flow.keyframe_asked(Instant::now());
                self.say(&ToHelper::Capture {
                    source: capture.source,
                    region: capture.region,
                    width: capture.width,
                    height: capture.height,
                    format: capture.format,
                    bitrate: self.flow.bitrate(),
                });
            }
            None => self.say(&ToHelper::Stop),
        }
    }

    fn wanted_capture(&self) -> Option<Capture> {
        if self.capture_failed_at.is_some() {
            return self.capture;
        }
        let source = self.source?;
        let (width, height, scale) = self.sources.size(source)?;
        let seat = self
            .control
            .as_ref()
            .and_then(|control| self.seats.get(control))
            .filter(|seat| seat.watching)?;
        let view = seat.size?;
        let whole = Region {
            x: 0.0,
            y: 0.0,
            width,
            height,
        };
        let look = seat.look.and_then(|look| self.within(look));
        // ScreenCaptureKit captures a window whole, so a closer look takes more of its pixels.
        let region = match source {
            Source::Display { .. } => look,
            Source::Window { .. } => None,
        };
        let (width, height) =
            capture_size(view, look.unwrap_or(whole), region.unwrap_or(whole), scale);
        Some(Capture {
            source,
            region,
            width,
            height,
            format: stream::choose(
                &self.encodable,
                self.seats.values().filter_map(Seat::viewing),
            ),
        })
    }

    fn publish(&mut self) {
        self.dirty = false;
        let (width, height) = self
            .source
            .and_then(|source| self.sources.size(source))
            .map_or((0.0, 0.0), |(width, height, _)| (width, height));
        for (viewer, seat) in self.seats.iter_mut().filter(|(_, seat)| seat.watching) {
            let shapes: Vec<Shape> = self
                .shapes
                .values()
                .filter(|shape| seat.shapes.insert(shape.id))
                .cloned()
                .collect();
            seat.mailbox.put(State {
                sources: (!std::mem::replace(&mut seat.has_sources, true))
                    .then(|| self.sources.clone()),
                source: self.source,
                width,
                height,
                pointer: self.pointer,
                shapes,
                access: self.access,
                control: self.control.as_deref() == Some(viewer),
            });
        }
    }
}

fn queued(seats: &HashMap<String, Seat>, viewer: &str) -> usize {
    seats.get(viewer).map_or(0, |seat| {
        seat.outgoing.max_capacity() - seat.outgoing.capacity()
    })
}

/// The pixels to capture of `captured` so that `looked`, the part the view shows, has as many
/// as the view, never more than the source has nor than the encoder takes, in even numbers.
fn capture_size(view: Size, looked: Region, captured: Region, pixels_per_point: f64) -> (u32, u32) {
    let shown = (
        f64::from(view.width) * f64::from(view.scale),
        f64::from(view.height) * f64::from(view.scale),
    );
    let (long, short) = if captured.width >= captured.height {
        (captured.width, captured.height)
    } else {
        (captured.height, captured.width)
    };
    let fit = (shown.0 / looked.width)
        .min(shown.1 / looked.height)
        .min(pixels_per_point)
        .min(MOST_PICTURE.0 / long)
        .min(MOST_PICTURE.1 / short);
    (even(captured.width * fit), even(captured.height * fit))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the sizes are small positive numbers"
)]
fn even(pixels: f64) -> u32 {
    ((pixels / 2.0).round().max(1.0) as u32) * 2
}

#[cfg(test)]
mod tests {
    use stream::viewer::{Input, KeyAction, MouseAction, MouseButton};
    use tokio::net::UnixStream;

    use super::*;
    use crate::link::Display;

    const HEVC: Decoder = Decoder {
        codec: Codec::Hevc,
        hardware: true,
    };

    struct Computer {
        helper: UnixStream,
        events: mpsc::Sender<Event>,
    }

    impl Computer {
        async fn start() -> Self {
            let (host, mut helper) = UnixStream::pair().unwrap();
            let (reader, writer) = host.into_split();
            let started = Started {
                reader,
                writer,
                encodable: vec![Codec::H264, Codec::Hevc],
                access: Access {
                    screen: true,
                    input: true,
                },
            };
            let calls = Calls {
                notice: Arc::new(|names| Notice {
                    watching: names.join(", "),
                    stop: "Stop".to_owned(),
                }),
                stopped: Arc::new(|| {}),
            };
            let (live, reader) = Live::new(started, Wanted::Screen, calls);
            let (events, receiver) = mpsc::channel(16);
            tokio::spawn(live.run(reader, receiver));
            let display = Display {
                id: 1,
                width: 1512.0,
                height: 982.0,
                scale: 2.0,
            };
            let sources = Sources {
                displays: vec![display],
                windows: Vec::new(),
            };
            helper
                .write_all(&link::json(&FromHelper::Sources { sources }))
                .await
                .unwrap();
            Self { helper, events }
        }

        async fn watch(&self, id: &str, size: Size, decoders: Vec<Decoder>, opened: bool) {
            let viewer = Viewer {
                id: id.to_owned(),
                person: "ana".to_owned(),
                name: "Ana".to_owned(),
                local: false,
            };
            let (outgoing, incoming) = mpsc::channel(64);
            std::mem::forget(incoming);
            let mailbox = Arc::new(Mailbox::default());
            self.events
                .send(Event::Join {
                    viewer,
                    seat: 1,
                    outgoing,
                    mailbox,
                })
                .await
                .unwrap();
            self.say(
                id,
                ToScreen::Watch {
                    size,
                    decoders,
                    anchors: false,
                    opened,
                },
            )
            .await;
        }

        async fn leave(&self, viewer: &str) {
            self.events
                .send(Event::Leave {
                    viewer: viewer.to_owned(),
                    seat: 1,
                })
                .await
                .unwrap();
        }

        async fn say(&self, viewer: &str, message: ToScreen) {
            self.events
                .send(Event::Message {
                    viewer: viewer.to_owned(),
                    message,
                })
                .await
                .unwrap();
        }

        /// The next message to the helper that `wanted` picks.
        async fn next<T>(&mut self, wanted: impl Fn(ToHelper) -> Option<T>) -> T {
            loop {
                let heard = link::read::<ToHelper>(&mut self.helper).await.unwrap();
                if let Some(Message::Said(said)) = heard
                    && let Some(found) = wanted(said)
                {
                    return found;
                }
            }
        }

        async fn capture(&mut self) -> (u32, u32, Codec) {
            self.next(|said| match said {
                ToHelper::Capture {
                    width,
                    height,
                    format,
                    ..
                } => Some((width, height, format.codec)),
                _ => None,
            })
            .await
        }
    }

    #[tokio::test]
    async fn the_capture_follows_the_view_that_acts_last_and_its_input_reaches_the_computer() {
        let mut computer = Computer::start().await;
        computer
            .watch("phone", view(390, 844, 3.0), vec![HEVC], true)
            .await;
        assert_eq!(computer.capture().await, (1170, 760, Codec::Hevc));

        computer
            .watch("laptop", view(1440, 900, 2.0), Vec::new(), false)
            .await;
        assert_eq!(computer.capture().await, (1170, 760, Codec::H264));

        let click = Input::Mouse {
            action: MouseAction::Press,
            x: 10.0,
            y: 20.0,
            button: MouseButton::Left,
            clicks: 1,
            delta_x: 0.0,
            delta_y: 0.0,
            modifiers: 0,
        };
        computer
            .say(
                "laptop",
                ToScreen::Input {
                    input: click.clone(),
                },
            )
            .await;
        assert_eq!(computer.capture().await, (2772, 1800, Codec::H264));
        let input = computer
            .next(|said| match said {
                ToHelper::Input { input } => Some(input),
                _ => None,
            })
            .await;
        assert_eq!(input, click);
    }

    #[tokio::test]
    async fn a_view_that_only_hovers_leaves_the_capture_to_the_view_that_acts() {
        let mut computer = Computer::start().await;
        computer
            .watch("phone", view(390, 844, 3.0), vec![HEVC], true)
            .await;
        computer.capture().await;
        computer
            .watch("laptop", view(1440, 900, 2.0), Vec::new(), false)
            .await;
        computer.capture().await;

        let hover = Input::Mouse {
            action: MouseAction::Move,
            x: 10.0,
            y: 20.0,
            button: MouseButton::None,
            clicks: 0,
            delta_x: 0.0,
            delta_y: 0.0,
            modifiers: 0,
        };
        computer
            .say("laptop", ToScreen::Input { input: hover })
            .await;
        computer
            .say(
                "phone",
                ToScreen::Size {
                    size: view(844, 390, 3.0),
                },
            )
            .await;
        assert_ne!(computer.capture().await, (2772, 1800, Codec::H264));
    }

    #[tokio::test]
    async fn a_viewer_that_goes_while_holding_a_button_and_a_key_lets_go_of_both() {
        let mut computer = Computer::start().await;
        computer
            .watch("laptop", view(1440, 900, 2.0), Vec::new(), true)
            .await;
        let command = Input::Key {
            action: KeyAction::Down,
            key: "Meta".to_owned(),
            code: "MetaLeft".to_owned(),
            text: None,
            key_code: 91,
            modifiers: 4,
        };
        let press = Input::Mouse {
            action: MouseAction::Press,
            x: 10.0,
            y: 20.0,
            button: MouseButton::Left,
            clicks: 1,
            delta_x: 0.0,
            delta_y: 0.0,
            modifiers: 4,
        };
        for input in [command, press] {
            computer.say("laptop", ToScreen::Input { input }).await;
        }
        computer.leave("laptop").await;
        let mut played = Vec::new();
        while played.len() < 4 {
            played.push(
                computer
                    .next(|said| match said {
                        ToHelper::Input { input } => Some(input),
                        _ => None,
                    })
                    .await,
            );
        }
        assert_eq!(
            played[2..],
            [
                Input::Mouse {
                    action: MouseAction::Release,
                    x: 10.0,
                    y: 20.0,
                    button: MouseButton::Left,
                    clicks: 1,
                    delta_x: 0.0,
                    delta_y: 0.0,
                    modifiers: 0,
                },
                Input::Key {
                    action: KeyAction::Up,
                    key: "Meta".to_owned(),
                    code: "MetaLeft".to_owned(),
                    text: None,
                    key_code: 91,
                    modifiers: 0,
                },
            ]
        );
    }

    #[tokio::test]
    async fn a_look_at_a_source_that_is_no_longer_shown_leaves_the_capture_as_it_is() {
        let mut computer = Computer::start().await;
        computer
            .watch("phone", view(390, 844, 3.0), vec![HEVC], true)
            .await;
        computer.capture().await;
        let part = Region {
            x: 100.0,
            y: 100.0,
            width: 400.0,
            height: 300.0,
        };
        for (source, region) in [
            (Source::Display { id: 2 }, region(200.0, 100.0)),
            (Source::Display { id: 1 }, part),
        ] {
            computer
                .say(
                    "phone",
                    ToScreen::Look {
                        source,
                        region: Some(region),
                    },
                )
                .await;
        }
        let region = computer
            .next(|said| match said {
                ToHelper::Capture { region, .. } => Some(region),
                _ => None,
            })
            .await;
        assert_eq!(region, Some(part));
    }

    fn region(width: f64, height: f64) -> Region {
        Region {
            x: 0.0,
            y: 0.0,
            width,
            height,
        }
    }

    fn view(width: u32, height: u32, scale: f32) -> Size {
        Size {
            width,
            height,
            scale,
        }
    }

    #[test]
    fn the_capture_has_the_pixels_that_the_view_shows_and_no_more_than_the_screen_has() {
        let screen = region(1512.0, 982.0);
        let whole = |view, screen, scale| capture_size(view, screen, screen, scale);
        assert_eq!(whole(view(390, 844, 3.0), screen, 2.0), (1170, 760));
        assert_eq!(whole(view(2000, 1300, 2.0), screen, 2.0), (3024, 1964));
        assert_eq!(
            whole(view(3000, 2000, 2.0), region(2560.0, 1440.0), 2.0),
            (4096, 2304)
        );
        assert_eq!(
            whole(view(390, 844, 3.0), region(200.0, 100.0), 2.0),
            (400, 200)
        );
        let window = region(800.0, 600.0);
        assert_eq!(
            capture_size(view(400, 300, 1.0), region(400.0, 300.0), window, 2.0),
            (800, 600)
        );
        assert_eq!(
            capture_size(view(400, 300, 2.0), region(200.0, 150.0), window, 2.0),
            (1600, 1200)
        );
    }
}
