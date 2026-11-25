//! The host's side of a video flow: frames out as shards, resends of what the viewer misses while
//! the frame can still arrive in time, and the bitrate that the stream may use.

use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use bytes::Bytes;
use serde::Serialize;
use stream::{media, viewer};
use tokio::{sync::mpsc, time::MissedTickBehavior};

use super::super::peers::Media;

const SEND_EVERY: Duration = Duration::from_millis(2);
const RATE_EVERY: Duration = Duration::from_millis(250);
/// A share that moved by more than this tells the stream at once.
const RATE_CHANGE: f64 = 0.05;
/// Frames waiting for the sender; more mean the path carries less than the encoder makes, and the
/// newest frames would only wait behind them.
const WAITING_FRAMES: usize = 4;
const LONGEST_PERIOD: Duration = Duration::from_millis(100);

/// A video pipe's flow on one connection, from the host.
pub(crate) struct Flow {
    pub(crate) number: u16,
    frames: mpsc::Sender<Bytes>,
    ready: Arc<AtomicBool>,
}

impl Flow {
    /// Starts a flow for a pipe on `media`; the stream hears through `document`, as from the
    /// viewer, the bitrate that its share of the path carries.
    pub(crate) fn start(media: &Media, document: mpsc::Sender<Vec<u8>>) -> Option<Self> {
        let (number, reports) = media.host()?;
        let (frames, waiting) = mpsc::channel(WAITING_FRAMES);
        let ready = Arc::new(AtomicBool::new(false));
        tokio::spawn(send(
            media.clone(),
            number,
            waiting,
            reports,
            document,
            Arc::clone(&ready),
        ));
        Some(Self {
            number,
            frames,
            ready,
        })
    }

    /// Takes a viewer message to send as datagrams, or returns it to go in the channel: JSON
    /// messages always do, and frames do until the viewer's first report arrives.
    pub(crate) fn take(&self, message: Vec<u8>) -> Result<(), Vec<u8>> {
        if !self.ready.load(Ordering::Acquire) || message.first() != Some(&viewer::FRAME) {
            return Err(message);
        }
        // A full queue drops the frame, and the viewer asks for a keyframe or anchor.
        drop(self.frames.try_send(Bytes::from(message)));
        Ok(())
    }
}

struct Waiting {
    shard: Bytes,
    since: Instant,
    until: Instant,
}

#[derive(Serialize)]
struct Rate {
    #[serde(rename = "type")]
    kind: &'static str,
    bitrate: u64,
}

async fn send(
    media: Media,
    number: u16,
    frames: mpsc::Receiver<Bytes>,
    reports: mpsc::Receiver<Bytes>,
    document: mpsc::Sender<Vec<u8>>,
    ready: Arc<AtomicBool>,
) {
    send_frames(media, number, frames, reports, &document, ready).await;
    // The viewer's frames go in the channel again, where the stream measures its own way.
    drop(document.try_send(rate(0).to_vec()));
}

fn rate(bitrate: u64) -> Bytes {
    viewer::json(&Rate {
        kind: "rate",
        bitrate,
    })
}

async fn send_frames(
    media: Media,
    number: u16,
    mut frames: mpsc::Receiver<Bytes>,
    mut reports: mpsc::Receiver<Bytes>,
    document: &mpsc::Sender<Vec<u8>>,
    ready: Arc<AtomicBool>,
) {
    let mut packer = media::Packer::new(number);
    let mut target = media::Target::default();
    let mut waiting = VecDeque::<Waiting>::new();
    let mut told: Option<(u64, Instant)> = None;
    let mut last_frame: Option<Instant> = None;
    let mut period = Duration::from_millis(16);
    let mut tick = tokio::time::interval(SEND_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            frame = frames.recv() => {
                let Some(frame) = frame else { return };
                let now = Instant::now();
                if let Some(last) = last_frame.replace(now) {
                    period = now.saturating_duration_since(last).min(LONGEST_PERIOD);
                }
                waiting.retain(|shard| shard.until > now);
                let queued = waiting
                    .front()
                    .map_or(Duration::ZERO, |oldest| now.saturating_duration_since(oldest.since));
                #[expect(clippy::cast_precision_loss, reason = "bitrates are far below 2^52")]
                let bitrate = told.map_or(0.0, |(bitrate, _)| bitrate as f64);
                target.frame(frame.len(), queued, period, bitrate);
                let until = now + media::frame_wait(media.rtt());
                let shards = packer.pack(&frame, media.datagram_bytes(), until, now);
                waiting.extend(shards.into_iter().map(|shard| Waiting { shard, since: now, until }));
            }
            report = reports.recv() => {
                let Some(report) = report else { return };
                ready.store(true, Ordering::Release);
                let now = Instant::now();
                let rtt = media.rtt();
                for shard in packer.report(&report, now, rtt).into_iter().rev() {
                    waiting.push_front(Waiting { shard, since: now, until: now + rtt });
                }
            }
            _ = tick.tick() => {}
        }
        let now = Instant::now();
        while let Some(next) = waiting.front() {
            if next.until > now && !media.send(next.shard.clone()) {
                break;
            }
            waiting.pop_front();
        }
        if ready.load(Ordering::Acquire)
            && let Some(carried) = media.carried().filter(|carried| *carried > 0)
        {
            let share = target.bitrate(carried) / u64::try_from(media.hosted().max(1)).unwrap_or(1);
            #[expect(clippy::cast_precision_loss, reason = "bitrates are far below 2^52")]
            let moved =
                |before: u64| (share as f64 - before as f64).abs() > RATE_CHANGE * before as f64;
            let due = told.is_none_or(|(before, at)| moved(before) || now - at >= RATE_EVERY);
            if due && document.try_send(rate(share).to_vec()).is_ok() {
                told = Some((share, now));
            }
        }
    }
}
