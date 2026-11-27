//! The host's side of a video flow: frames out as shards as fast as the connection takes them,
//! under the time, drop and resend rules of `stream::media`, word to the stream of each frame
//! that cannot arrive, and the bitrate that the stream may use.

use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use futures_util::FutureExt as _;
use serde::Serialize;
use stream::{media, viewer};
use tokio::{sync::mpsc, time::Instant};

use super::super::peers::Media;

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
    /// Frames that the send task had no room for, which it tells the stream of.
    unsent: mpsc::UnboundedSender<u32>,
    ready: Arc<AtomicBool>,
}

impl Flow {
    /// Starts a flow for a pipe on `media`; the stream hears through `document`, as from the
    /// viewer, the bitrate that its share of the path carries and each frame that cannot arrive.
    pub(crate) fn start(media: &Media, document: mpsc::Sender<Vec<u8>>) -> Option<Self> {
        let (number, reports) = media.host()?;
        let (frames, waiting) = mpsc::channel(WAITING_FRAMES);
        let (unsent, dropped) = mpsc::unbounded_channel();
        let ready = Arc::new(AtomicBool::new(false));
        let pipes = Pipes {
            frames: waiting,
            reports,
            dropped,
            document,
        };
        tokio::spawn(send(media.clone(), number, pipes, Arc::clone(&ready)));
        Some(Self {
            number,
            frames,
            unsent,
            ready,
        })
    }

    /// Takes a viewer message to send as datagrams, or returns it to go in the channel: JSON
    /// messages always do, and frames do until the viewer's first report arrives and once the
    /// flow ended.
    pub(crate) fn take(&self, message: Vec<u8>) -> Result<(), Vec<u8>> {
        if !self.ready.load(Ordering::Acquire) || message.first() != Some(&viewer::FRAME) {
            return Err(message);
        }
        let id = media::frame_id(&message);
        match self.frames.try_send(Bytes::from(message)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(frame)) => match id {
                Some(id) if self.unsent.send(id).is_err() => Err(frame.into()),
                _ => Ok(()),
            },
            Err(mpsc::error::TrySendError::Closed(frame)) => Err(frame.into()),
        }
    }
}

/// What the send task hears from and tells.
struct Pipes {
    frames: mpsc::Receiver<Bytes>,
    reports: mpsc::Receiver<Bytes>,
    /// Frames that `Flow::take` had no room for.
    dropped: mpsc::UnboundedReceiver<u32>,
    /// The stream, which takes these messages as from the viewer.
    document: mpsc::Sender<Vec<u8>>,
}

#[derive(Serialize)]
struct Rate {
    #[serde(rename = "type")]
    kind: &'static str,
    bitrate: u64,
}

#[derive(Serialize)]
struct Lost {
    #[serde(rename = "type")]
    kind: &'static str,
    frame: u32,
}

async fn send(media: Media, number: u16, mut pipes: Pipes, ready: Arc<AtomicBool>) {
    let mut packer = media::Packer::new(number);
    // The stream hears of each frame that cannot arrive, however long it takes to make room:
    // without the word it would wait for the frame for good.
    let mut owed = VecDeque::new();
    send_frames(&media, &mut packer, &mut pipes, &mut owed, &ready).await;
    // From here on frames go in the channel; those that the flow took cannot arrive: no report
    // comes any more for the frames that the viewer did not report on, and those that wait were
    // never sent.
    ready.store(false, Ordering::Release);
    pipes.frames.close();
    pipes.dropped.close();
    owed.extend(packer.end());
    while let Ok(frame) = pipes.frames.try_recv() {
        owed.extend(media::frame_id(&frame));
    }
    while let Ok(id) = pipes.dropped.try_recv() {
        owed.push_back(id);
    }
    for id in owed {
        if pipes.document.send(lost(id)).await.is_err() {
            return;
        }
    }
    // The viewer's frames go in the channel again, where the stream measures its own way.
    drop(pipes.document.send(rate(0).to_vec()).await);
}

fn rate(bitrate: u64) -> Bytes {
    viewer::json(&Rate {
        kind: "rate",
        bitrate,
    })
}

/// The word that the viewer cannot have frame `id`, so that the stream repairs the picture at once.
fn lost(id: u32) -> Vec<u8> {
    viewer::json(&Lost {
        kind: "lost",
        frame: id,
    })
    .to_vec()
}

/// Waits until the connection has room for `shard`, or forever without one.
async fn when_room(media: &Media, shard: Option<Bytes>) -> bool {
    match shard {
        Some(shard) => media.send_when_room(shard).await,
        None => std::future::pending().await,
    }
}

async fn send_frames(
    media: &Media,
    packer: &mut media::Packer,
    pipes: &mut Pipes,
    owed: &mut VecDeque<u32>,
    ready: &AtomicBool,
) {
    let mut target = media::Target::default();
    let mut told: Option<(u64, Instant)> = None;
    let mut last_frame: Option<Instant> = None;
    let mut period = Duration::from_millis(16);
    loop {
        let now = Instant::now();
        // The connection's queue takes shards while it has room; the next one waits for room.
        // `datagram_send_buffer_space` counts one datagram's overhead for all that wait, so only
        // the send itself tells whether one more fits without pushing out another.
        let waiting = loop {
            let Some(shard) = packer.next(now.into_std()) else {
                break None;
            };
            match media.send_when_room(shard.clone()).now_or_never() {
                Some(true) => packer.sent(now.into_std()),
                Some(false) => return,
                None => break Some(shard),
            }
        };
        // A still picture leaves only the rate to tell.
        let wake = packer.wake().map_or(now + RATE_EVERY, |at| {
            Instant::from_std(at).min(now + RATE_EVERY)
        });
        tokio::select! {
            frame = pipes.frames.recv() => {
                let Some(frame) = frame else { return };
                let now = Instant::now();
                if let Some(last) = last_frame.replace(now) {
                    period = now.saturating_duration_since(last).min(LONGEST_PERIOD);
                }
                #[expect(clippy::cast_precision_loss, reason = "bitrates are far below 2^52")]
                let bitrate = told.map_or(0.0, |(bitrate, _)| bitrate as f64);
                target.frame(frame.len(), packer.waited(now.into_std()), period, bitrate);
                packer.pack(&frame, media.datagram_bytes(), media.link(), now.into_std());
            }
            report = pipes.reports.recv() => {
                let Some(report) = report else { return };
                let now = Instant::now();
                ready.store(true, Ordering::Release);
                packer.report(&report, now.into_std(), media.rtt());
            }
            sent = when_room(media, waiting) => {
                if !sent {
                    return;
                }
                packer.sent(Instant::now().into_std());
            }
            Some(id) = pipes.dropped.recv() => owed.push_back(id),
            room = pipes.document.reserve(), if !owed.is_empty() => {
                let Ok(room) = room else { return };
                if let Some(id) = owed.pop_front() {
                    room.send(lost(id));
                }
            }
            () = tokio::time::sleep_until(wake) => {}
        }
        owed.extend(packer.lost());
        let now = Instant::now();
        if ready.load(Ordering::Acquire)
            && let Some(carried) = media.carried().filter(|carried| *carried > 0)
        {
            let share = target.bitrate(carried) / u64::try_from(media.hosted().max(1)).unwrap_or(1);
            #[expect(clippy::cast_precision_loss, reason = "bitrates are far below 2^52")]
            let moved =
                |before: u64| (share as f64 - before as f64).abs() > RATE_CHANGE * before as f64;
            let due = told.is_none_or(|(before, at)| moved(before) || now - at >= RATE_EVERY);
            if due && pipes.document.try_send(rate(share).to_vec()).is_ok() {
                told = Some((share, now));
            }
        }
    }
}
