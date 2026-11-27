//! The video frames of browser and screen pipes as QUIC datagrams on the connection under a lane,
//! so that a lost packet delays only its own frame (see `media` in terminal-connections.json).

#[cfg(feature = "host")]
mod send;

use std::time::{Duration, Instant};

use bytes::Bytes;
use stream::media;
use tokio::{sync::mpsc, task::AbortHandle, time::MissedTickBehavior};

#[cfg(feature = "host")]
pub(super) use send::Flow;

use super::peers::Media;

const RECEIVE_EVERY: Duration = Duration::from_millis(5);
const GREET_EVERY: Duration = Duration::from_millis(200);

/// Takes the datagrams of a flow that the host offered for a pipe, and hands its frames on to the
/// pipe's consumer.
pub(super) fn receive(
    media: &Media,
    flow: u16,
    frames: mpsc::Sender<Vec<u8>>,
) -> Option<AbortHandle> {
    let shards = media.view(flow)?;
    let task = tokio::spawn(unpack(media.clone(), flow, shards, frames));
    Some(task.abort_handle())
}

async fn unpack(
    media: Media,
    flow: u16,
    mut shards: mpsc::Receiver<Bytes>,
    frames: mpsc::Sender<Vec<u8>>,
) {
    let mut unpacker = media::Unpacker::new(flow);
    let greeting = media::greeting(flow);
    media.report(greeting.clone());
    let mut greeted_at = Instant::now();
    let mut heard = false;
    let mut tick = tokio::time::interval(RECEIVE_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        // A still picture sends nothing, so once the host answered and nothing waits to be put
        // together or reported, the viewer sleeps until a shard comes.
        let ticking = !heard || !unpacker.idle();
        let handed = tokio::select! {
            shard = shards.recv() => {
                let Some(shard) = shard else { return };
                if !ticking {
                    tick.reset();
                }
                heard = true;
                unpacker.shard(&shard, Instant::now())
            }
            _ = tick.tick(), if ticking => {
                let now = Instant::now();
                if !heard && now.saturating_duration_since(greeted_at) >= GREET_EVERY {
                    media.report(greeting.clone());
                    greeted_at = now;
                }
                let (handed, report) = unpacker.tick(now, media.rtt(), media.datagram_bytes());
                if let Some(report) = report {
                    media.report(report);
                }
                handed
            }
        };
        for frame in handed {
            // A frame put together reaches the app, which acknowledges it, however long the app
            // takes: meanwhile the flow's shards wait in its queue or are dropped there, and the
            // host hears that those frames did not arrive.
            if frames.send(frame.to_vec()).await.is_err() {
                return;
            }
        }
    }
}
