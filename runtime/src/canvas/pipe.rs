//! Carries a viewer's pipe to a live view on the canvas: the shared browser or the screen.

use std::future::Future;

use bytes::Bytes;
use tokio::sync::mpsc;

use super::{PIPE_QUEUE, PipeEnds};

/// A live view's end of a viewer's pipe.
pub(super) trait Viewed: Send + 'static {
    /// Returns false once the view ended.
    fn send(&self, message: &[u8]) -> impl Future<Output = bool> + Send;
    fn receive(&mut self) -> impl Future<Output = Option<Bytes>> + Send;
}

pub(super) fn carry(mut viewed: impl Viewed) -> PipeEnds {
    let (sender, mut from_view) = mpsc::channel::<Vec<u8>>(PIPE_QUEUE);
    let (to_view, receiver) = mpsc::channel(PIPE_QUEUE);
    tokio::spawn(async move {
        // Hold one pending message when `to_view` is full and pause `receive()`, so that the view
        // slows down upstream: a slow viewer gets fewer frames instead of broken ones.
        let mut waiting: Option<Vec<u8>> = None;
        loop {
            tokio::select! {
                message = from_view.recv() => {
                    let Some(message) = message else { break };
                    if !viewed.send(&message).await {
                        break;
                    }
                }
                room = to_view.reserve(), if waiting.is_some() => {
                    let Ok(room) = room else { break };
                    if let Some(message) = waiting.take() {
                        room.send(message);
                    }
                }
                message = viewed.receive(), if waiting.is_none() => {
                    let Some(message) = message else { break };
                    match to_view.try_send(message.to_vec()) {
                        Ok(()) => {}
                        Err(mpsc::error::TrySendError::Full(message)) => waiting = Some(message),
                        Err(mpsc::error::TrySendError::Closed(_)) => break,
                    }
                }
            }
        }
    });
    PipeEnds { sender, receiver }
}
