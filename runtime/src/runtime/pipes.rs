use std::time::Duration;

use base64::Engine as _;
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{Completion, Runtime, Scope};
use crate::{
    Command, Error, Result,
    canvas::PipeEnds,
    protocol::{SessionStatus, parse_id},
    terminal::SessionChange,
    text::Text,
};

const MOST_PIPES: usize = 32;
/// A break of the link that ends within this time shows no sign.
const PATIENCE: Duration = Duration::from_millis(1500);
const LONGEST_WAIT: Duration = Duration::from_secs(30);

/// A pipe of a document or of the browser that the runtime keeps for its client. It opens the
/// pipe again after each loss: at once when the terminal runs again, and after a wait that grows
/// while the terminal runs and the pipe fails.
pub(super) struct Kept {
    session: Uuid,
    key: String,
    consumer: Uuid,
    lifetime: CancellationToken,
    news: mpsc::UnboundedSender<News>,
    state: State,
    failures: u32,
    /// The last part from the client did not end its frame.
    within_frame: bool,
    opened: bool,
    told: Told,
}

/// What the client last heard about a pipe that is not open, since it was open.
#[derive(PartialEq, Eq)]
enum Told {
    Nothing,
    Down(Option<Text>),
}

enum State {
    Waiting {
        at: Instant,
        for_terminal: bool,
    },
    Opening(Uuid),
    Open {
        attempt: Uuid,
        sender: mpsc::Sender<Vec<u8>>,
        filling: Vec<u8>,
        /// The client began a frame before the pipe opened, so its rest goes nowhere.
        skipping: bool,
    },
}

enum News {
    Open {
        attempt: Uuid,
        frames: mpsc::Receiver<Vec<u8>>,
        scope: Scope,
    },
    Down {
        reason: Option<Text>,
        scope: Scope,
    },
    Closed {
        scope: Scope,
    },
}

impl Kept {
    fn tell(&self, news: News) {
        drop(self.news.send(news));
    }

    fn wait(&mut self, failed: bool) {
        self.failures = if failed {
            self.failures.saturating_add(1)
        } else {
            0
        };
        self.state = State::Waiting {
            at: Instant::now() + retry_wait(self.failures),
            for_terminal: false,
        };
    }

    /// A pipe that never opened has no loss to tell, only a reason why it does not open.
    fn fall(&mut self, reason: Option<Text>, scope: &Scope) {
        if reason.is_none() && !self.opened {
            return;
        }
        let told = Told::Down(reason.clone());
        if self.told != told {
            self.told = told;
            self.tell(News::Down {
                reason,
                scope: scope.clone(),
            });
        }
    }

    fn waits_for(&self, attempt: Uuid) -> bool {
        matches!(self.state, State::Opening(opening) if opening == attempt)
    }

    fn carries(&self, attempt: Uuid) -> bool {
        matches!(self.state, State::Open { attempt: open, .. } if open == attempt)
    }
}

fn retry_wait(failures: u32) -> Duration {
    match failures {
        0 => Duration::ZERO,
        failures => Duration::from_secs(1)
            .saturating_mul(1 << (failures - 1).min(5))
            .min(LONGEST_WAIT),
    }
}

impl Runtime {
    pub(super) fn open_document(
        &mut self,
        command: &Command,
        consumer: Uuid,
        lifetime: CancellationToken,
    ) -> Result<()> {
        let Command::DocumentOpen {
            session_id,
            pipe,
            key,
            ..
        } = command
        else {
            return Err(Error::Invalid("expected a document".to_owned()));
        };
        let session = parse_id(session_id)?;
        let pipe = parse_id(pipe)?;
        if !self.listed(&session) {
            return Err(Error::NotFound);
        }
        self.prune_pipes();
        if self.pipes.contains_key(&pipe) {
            return Err(Error::Invalid("The pipe is already open.".to_owned()));
        }
        if self.pipes.len() >= MOST_PIPES {
            tracing::warn!(
                pipes = MOST_PIPES,
                "the host keeps the most documents open, so a view waits"
            );
            return Err(Error::Busy);
        }
        let (news, heard) = mpsc::unbounded_channel();
        let carrier = Carrier {
            events: self.events.clone(),
            direct: self
                .direct
                .get(&consumer)
                .filter(|direct| !direct.is_closed())
                .cloned(),
            consumer,
        };
        tokio::spawn(
            Delivery {
                pipe,
                carrier,
                scope: self.scope.clone(),
                news: heard,
                ended: self.changes.clone(),
                frames: None,
                carried: false,
                sequence: 0,
                lost_at: None,
                reason: None,
                told: false,
            }
            .run(),
        );
        self.pipes.insert(
            pipe,
            Kept {
                session,
                key: key.clone(),
                consumer,
                lifetime,
                news,
                state: State::Waiting {
                    at: Instant::now(),
                    for_terminal: false,
                },
                failures: 0,
                within_frame: false,
                opened: false,
                told: Told::Nothing,
            },
        );
        self.result(command);
        self.keep_pipes();
        Ok(())
    }

    pub(super) fn close_document(&mut self, command: &Command) -> Result<()> {
        let Command::DocumentClose { pipe, .. } = command else {
            return Err(Error::Invalid("expected a document".to_owned()));
        };
        self.pipes.remove(&parse_id(pipe)?);
        Ok(())
    }

    pub(super) fn send_to_document(&mut self, command: &Command) -> Result<()> {
        let Command::DocumentSend {
            pipe, more, data, ..
        } = command
        else {
            return Err(Error::Invalid("expected a document frame".to_owned()));
        };
        let id = parse_id(pipe)?;
        let part = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| Error::Invalid("The document part is not valid.".to_owned()))?;
        let scope = self.scope.clone();
        let kept = self.pipes.get_mut(&id).ok_or(Error::NotFound)?;
        kept.within_frame = *more;
        let State::Open {
            sender,
            filling,
            skipping,
            ..
        } = &mut kept.state
        else {
            return Ok(());
        };
        if *skipping {
            *skipping = *more;
            return Ok(());
        }
        let failure = if filling.len() + part.len() > crate::canvas::MOST_PIPE_FRAME {
            Some(Error::Invalid("The document frame is too long.".to_owned()))
        } else {
            filling.extend_from_slice(&part);
            (!*more && sender.try_send(std::mem::take(filling)).is_err()).then_some(Error::Busy)
        };
        let Some(failure) = failure else {
            return Ok(());
        };
        kept.wait(true);
        kept.fall(None, &scope);
        Err(failure)
    }

    pub(super) fn prune_pipes(&mut self) {
        self.pipes.retain(|_, kept| !kept.lifetime.is_cancelled());
    }

    /// Each pipe opens again under the new account, and a pipe whose terminal the account does
    /// not list closes.
    pub(super) fn reopen_pipes(&mut self) {
        let scope = self.scope.clone();
        for kept in self.pipes.values_mut() {
            kept.wait(false);
            kept.told = Told::Nothing;
            kept.fall(None, &scope);
        }
    }

    pub(super) fn keep_pipes(&mut self) {
        self.prune_pipes();
        let scope = self.scope.clone();
        let now = Instant::now();
        let pipes = self.pipes.keys().copied().collect::<Vec<_>>();
        for pipe in pipes {
            let Some(session) = self.pipes.get(&pipe).map(|kept| kept.session) else {
                continue;
            };
            if !self.listed(&session) {
                if let Some(kept) = self.pipes.remove(&pipe) {
                    kept.tell(News::Closed {
                        scope: scope.clone(),
                    });
                }
                continue;
            }
            let runs = self.runs(session);
            let reason =
                (!runs && !self.host_online(session)).then(|| self.offline(session).text());
            let Some(kept) = self.pipes.get_mut(&pipe) else {
                continue;
            };
            let State::Waiting { at, for_terminal } = kept.state else {
                continue;
            };
            if !runs {
                kept.state = State::Waiting {
                    at,
                    for_terminal: true,
                };
                kept.fall(reason, &scope);
            } else if for_terminal || at <= now {
                if for_terminal {
                    kept.failures = 0;
                }
                self.open_kept(pipe);
            }
        }
    }

    /// The terminal can take a pipe: a remote one wakes for it when it sleeps.
    fn runs(&self, id: Uuid) -> bool {
        self.target(id).is_ok()
            && (self.hosts(&id)
                || self.remote_status(id, self.host_online(id)) == SessionStatus::Running)
    }

    fn open_kept(&mut self, pipe: Uuid) {
        let Some(kept) = self.pipes.get(&pipe) else {
            return;
        };
        let opening = self
            .job_capacity()
            .and_then(|()| self.target(kept.session))
            .map(|target| {
                let attempt = Uuid::now_v7();
                let name = self.person.clone().unwrap_or_else(|| "Someone".to_owned());
                let opening = target.document(
                    attempt,
                    kept.key.clone(),
                    name,
                    self.scope.user.clone(),
                    kept.consumer,
                );
                (attempt, opening)
            });
        let Some(kept) = self.pipes.get_mut(&pipe) else {
            return;
        };
        match opening {
            Ok((attempt, opening)) => {
                kept.state = State::Opening(attempt);
                self.spawn(async move {
                    Completion::Document {
                        pipe,
                        attempt,
                        result: opening.await,
                    }
                });
            }
            Err(_) => {
                kept.state = State::Waiting {
                    at: Instant::now() + Duration::from_secs(1),
                    for_terminal: false,
                };
            }
        }
    }

    pub(super) fn document_opened(
        &mut self,
        pipe: Uuid,
        attempt: Uuid,
        result: Result<PipeEnds>,
        current: bool,
    ) {
        let scope = self.scope.clone();
        let Some(kept) = self
            .pipes
            .get_mut(&pipe)
            .filter(|kept| kept.waits_for(attempt))
        else {
            return;
        };
        match result {
            Ok(ends) if current => {
                kept.state = State::Open {
                    attempt,
                    sender: ends.sender,
                    filling: Vec::new(),
                    skipping: kept.within_frame,
                };
                kept.opened = true;
                kept.told = Told::Nothing;
                kept.tell(News::Open {
                    attempt,
                    frames: ends.receiver,
                    scope,
                });
            }
            Ok(_) => kept.wait(false),
            Err(error) => {
                kept.wait(true);
                kept.fall(Some(error.text()), &scope);
            }
        }
    }

    /// A pipe that ends before it carried a frame counts as a failed one, so a terminal that
    /// refuses a document is not asked again at once.
    pub(super) fn document_ended(&mut self, pipe: Uuid, attempt: Uuid, carried: bool) {
        if let Some(kept) = self
            .pipes
            .get_mut(&pipe)
            .filter(|kept| kept.carries(attempt))
        {
            kept.wait(!carried);
        }
    }

    pub(super) fn pipes_due(&self) -> Option<Instant> {
        self.pipes
            .values()
            .filter_map(|kept| match kept.state {
                State::Waiting {
                    at,
                    for_terminal: false,
                } => Some(at),
                _ => None,
            })
            .min()
    }
}

struct Carrier {
    events: broadcast::Sender<crate::Event>,
    direct: Option<mpsc::Sender<crate::Event>>,
    consumer: Uuid,
}

impl Carrier {
    async fn send(&self, scope: &Scope, value: Value) -> bool {
        let Ok(mut event) = crate::Event::new(scope.user.clone(), scope.epoch, value) else {
            return true;
        };
        event.recipient = Some(self.consumer);
        if let Some(direct) = &self.direct {
            return direct.send(event).await.is_ok();
        }
        drop(self.events.send(event));
        true
    }
}

/// The client hears each pipe in one order: it opened, its frames, and that it stayed lost.
struct Delivery {
    pipe: Uuid,
    carrier: Carrier,
    scope: Scope,
    news: mpsc::UnboundedReceiver<News>,
    ended: mpsc::Sender<SessionChange>,
    frames: Option<(Uuid, mpsc::Receiver<Vec<u8>>)>,
    carried: bool,
    sequence: u64,
    lost_at: Option<Instant>,
    reason: Option<Text>,
    told: bool,
}

impl Delivery {
    async fn run(mut self) {
        loop {
            let patience = self.lost_at.filter(|_| !self.told).map(|at| at + PATIENCE);
            let going = tokio::select! {
                news = self.news.recv() => match news {
                    Some(news) => self.heard(news).await,
                    None => false,
                },
                frame = next_frame(&mut self.frames) => match frame {
                    Some(frame) => self.carry(frame).await,
                    None => self.lost().await,
                },
                () = tokio::time::sleep_until(patience.unwrap_or_else(Instant::now)), if patience.is_some() => {
                    self.tell_lost().await
                }
            };
            if !going {
                return;
            }
        }
    }

    async fn heard(&mut self, news: News) -> bool {
        match news {
            News::Open {
                attempt,
                frames,
                scope,
            } => {
                self.scope = scope;
                self.frames = Some((attempt, frames));
                self.carried = false;
                self.sequence = 0;
                self.send(json!({ "type": "document.opened", "pipe": self.pipe }))
                    .await
            }
            News::Down { reason, scope } => {
                self.scope = scope;
                self.frames = None;
                self.lost_at.get_or_insert_with(Instant::now);
                let changed = self.reason != reason;
                self.reason = reason;
                !(self.told && changed) || self.tell_lost().await
            }
            News::Closed { scope } => {
                self.scope = scope;
                self.send(json!({ "type": "document.closed", "pipe": self.pipe }))
                    .await;
                false
            }
        }
    }

    async fn carry(&mut self, frame: Vec<u8>) -> bool {
        if !self.carried {
            self.carried = true;
            self.lost_at = None;
            self.reason = None;
            self.told = false;
        }
        let parts = frame.len().div_ceil(crate::canvas::PART_BYTES);
        for (index, part) in frame.chunks(crate::canvas::PART_BYTES).enumerate() {
            let sent = self
                .send(json!({
                    "type": "document.frame",
                    "pipe": self.pipe,
                    "sequence": self.sequence,
                    "more": index + 1 < parts,
                    "data": base64::engine::general_purpose::STANDARD.encode(part),
                }))
                .await;
            if !sent {
                return false;
            }
            self.sequence += 1;
        }
        true
    }

    async fn lost(&mut self) -> bool {
        let Some((attempt, _)) = self.frames.take() else {
            return true;
        };
        self.lost_at.get_or_insert_with(Instant::now);
        self.ended
            .send(SessionChange::PipeEnded {
                pipe: self.pipe,
                attempt,
                carried: self.carried,
            })
            .await
            .is_ok()
    }

    async fn tell_lost(&mut self) -> bool {
        self.told = true;
        self.send(json!({
            "type": "document.lost",
            "pipe": self.pipe,
            "message": self.reason,
        }))
        .await
    }

    async fn send(&self, value: Value) -> bool {
        self.carrier.send(&self.scope, value).await
    }
}

async fn next_frame(frames: &mut Option<(Uuid, mpsc::Receiver<Vec<u8>>)>) -> Option<Vec<u8>> {
    match frames {
        Some((_, frames)) => frames.recv().await,
        None => std::future::pending().await,
    }
}
