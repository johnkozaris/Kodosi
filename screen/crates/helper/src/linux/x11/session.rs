//! The link to the host on X11: what it asks of the capture, the input that it passes on, and what
//! the helper tells it back.

use std::sync::Arc;

use bytes::Bytes;
use screen::link::{
    self, Access, Away, Display, FromHelper, Notice, Source, Sources, ToHelper, VERSION,
};
use stream::{Region, viewer::Input};
use stream_encode::{Encoder, Settings};
use tokio::sync::mpsc;

use super::{
    Monitor,
    capture::{Capture, Wanted},
    desk::Desk,
};
use crate::linux::{
    Told,
    away::Watch,
    frames::Frames,
    hand::Hand,
    notice::Tray,
    session::{FIRST_LOOK, LOOK_EVERY, MESSAGES, lets_go, read, same_size, write},
};

pub(super) enum Event {
    /// The person at the computer stopped the sharing in the notice.
    Stopped,
    /// The lock, the account in front of the display, or the displays changed.
    Changed,
    /// The capture or its encoder failed.
    Failed(String),
    /// The encoder needs the newest picture again.
    Redraw,
    /// Text copied on the computer.
    Copied(String),
}

impl Told for Event {
    fn stopped() -> Self {
        Self::Stopped
    }

    fn changed() -> Self {
        Self::Changed
    }

    fn failed(problem: String) -> Self {
        Self::Failed(problem)
    }

    fn redraw() -> Self {
        Self::Redraw
    }
}

/// Serves the host on standard input and output until it quits or goes away.
pub(in crate::linux) async fn serve(token: String) {
    let (say, saying) = mpsc::channel(MESSAGES);
    tokio::spawn(write(saying));
    let (events, hearing) = mpsc::unbounded_channel();
    let hand = Arc::<Hand>::default();
    let (desk, watch) = tokio::join!(
        Desk::open(events.clone(), Arc::clone(&hand)),
        tokio::time::timeout(FIRST_LOOK, Watch::start(events.clone())),
    );
    let mut session = Session {
        say,
        events: events.clone(),
        desk,
        watch: watch.ok(),
        tray: Tray::start(events),
        hand,
        away: None,
        watched: false,
        monitors: Vec::new(),
        sources: Sources::default(),
        capture: None,
        frames: None,
        copied: None,
    };
    session.hello(token).await;
    session.run(hearing).await;
}

struct Session {
    say: mpsc::Sender<Bytes>,
    events: mpsc::UnboundedSender<Event>,
    /// Viewers' hands at the X server, or why the X server cannot be reached.
    desk: Result<Desk, String>,
    watch: Option<Watch>,
    tray: Tray,
    hand: Arc<Hand>,
    away: Option<Away>,
    watched: bool,
    /// The displays as the helper last looked, the main one first.
    monitors: Vec<Monitor>,
    sources: Sources,
    capture: Option<Capture>,
    /// The encoder, from the first capture until the host stops it.
    frames: Option<Arc<Frames>>,
    /// The text that last went between the computer and a viewer, which a viewer's device or a
    /// clipboard shared between devices brings back.
    copied: Option<String>,
}

impl Session {
    /// X11 asks nothing of the person at the computer before a program records the screen or
    /// plays input.
    async fn hello(&mut self, token: String) {
        self.away = self.watch.as_ref().and_then(Watch::away);
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
                if let Ok(desk) = &self.desk {
                    desk.put(text).await;
                }
            }
            ToHelper::Ask | ToHelper::Forget => {}
            // Every program of the person's session sees and controls the screen anyway.
            ToHelper::Owner { .. } => self.tell(FromHelper::Owner { allowed: true }).await,
            ToHelper::Allow { .. } => {
                self.tell(FromHelper::Allowed {
                    allowed: true,
                    problem: None,
                })
                .await;
            }
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
        let monitor = display(&self.monitors, id).ok_or("the display is gone")?;
        if !same_size(monitor.size(), size) {
            return Ok(());
        }
        let desk = self.desk.as_ref().map_err(Clone::clone)?;
        let frames =
            Arc::clone(self.frames.get_or_insert_with(|| {
                Frames::new(settings, self.say.clone(), self.events.clone())
            }));
        frames.encode_as(settings);
        let wanted = Wanted {
            monitor,
            region,
            width: pixels.0,
            height: pixels.1,
        };
        if let Some(capture) = self
            .capture
            .as_ref()
            .filter(|capture| capture.display == id)
        {
            capture.update(wanted);
            return Ok(());
        }
        if let Some(earlier) = self.capture.take() {
            earlier.stop().await;
        } else {
            desk.copies(true);
        }
        let started = Capture::start(
            id,
            wanted,
            frames,
            self.say.clone(),
            self.events.clone(),
            Arc::clone(&self.hand),
        );
        self.capture = Some(started.await?);
        Ok(())
    }

    async fn stop(&mut self) {
        if let Some(capture) = self.capture.take() {
            capture.stop().await;
        }
        if let Ok(desk) = &self.desk {
            desk.copies(false);
            desk.let_go();
        }
        self.frames = None;
    }

    /// Stops the capture after it failed; the host asks again, maybe for another display.
    async fn fail(&mut self, problem: String) {
        self.stop().await;
        self.tell(FromHelper::Failed { problem }).await;
        self.look().await;
    }

    fn encoder(&self, work: impl FnOnce(&mut Encoder)) {
        if let Some(frames) = &self.frames {
            frames.encoder(work);
        }
    }

    fn play(&self, input: Input) {
        if self.hand.busy() && !lets_go(&input) {
            return;
        }
        let (Some(capture), Ok(desk)) = (&self.capture, &self.desk) else {
            return;
        };
        if let Some(monitor) = display(&self.monitors, capture.display) {
            desk.play(input, monitor.origin());
        }
    }

    async fn notice(&mut self, notice: Option<Notice>) {
        let first = notice.is_some() && !self.watched;
        self.watched = notice.is_some();
        self.tray.show(notice);
        if !self.watched {
            self.stop().await;
            return;
        }
        // Opening the screen wakes its display.
        if first && let Some(watch) = &self.watch {
            watch.wake();
        }
    }

    async fn event(&mut self, event: Event) {
        match event {
            Event::Stopped => {
                self.stop().await;
                self.tray.show(None);
                self.tell(FromHelper::Stopped).await;
            }
            Event::Changed => self.look().await,
            Event::Failed(problem) => {
                if self.capture.is_some() {
                    self.fail(problem).await;
                }
            }
            Event::Redraw => {
                if let Some(capture) = &self.capture {
                    capture.redraw();
                }
            }
            Event::Copied(text) => {
                if self.capture.is_some() && self.copied.as_ref() != Some(&text) {
                    self.copied = Some(text.clone());
                    self.tell(FromHelper::Copied { text }).await;
                }
            }
        }
    }

    /// Tells the host what changed, the sources before the screen comes back, so that it
    /// captures what is there now.
    async fn look(&mut self) {
        let away = self.watch.as_ref().and_then(Watch::away);
        if away.is_none()
            && let Ok(desk) = &self.desk
        {
            let monitors = desk.monitors().await;
            if !monitors.is_empty() {
                let sources = sources(&monitors);
                self.monitors = monitors;
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

    async fn tell(&self, message: FromHelper) {
        drop(self.say.send(link::json(&message)).await);
    }
}

fn sources(monitors: &[Monitor]) -> Sources {
    Sources {
        displays: (1..)
            .zip(monitors)
            .map(|(id, monitor)| Display {
                id,
                width: f64::from(monitor.width),
                height: f64::from(monitor.height),
                scale: 1.0,
            })
            .collect(),
        windows: Vec::new(),
    }
}

/// Displays have the ids 1, 2, … in `RandR`'s order, the main one first.
fn display(monitors: &[Monitor], id: u32) -> Option<Monitor> {
    monitors
        .get(usize::try_from(id.checked_sub(1)?).ok()?)
        .copied()
}
