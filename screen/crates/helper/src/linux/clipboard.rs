//! The clipboard through the portal session, where the desktop offers it: text copied on the
//! computer goes to the viewer in control, and a viewer's text comes onto the computer's
//! clipboard before it pastes there.

use std::{collections::HashMap, os::fd::OwnedFd, time::Duration};

use futures_util::StreamExt as _;
use screen::link::MOST_CLIPBOARD;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::unix::pipe,
    sync::{mpsc, oneshot},
};
use zbus::{
    DeserializeDict, DynamicType, MatchRule, MessageStream, OwnedObjectPath, Type, Variant,
    message::Type as MessageType, names::MemberName,
};

use super::{
    portal::{CLIPBOARD, DESKTOP, DESKTOP_PATH, Desktop, Problem},
    session::Event,
};

/// The kinds of text that the helper offers and reads, the best first.
const TEXT: [&str; 5] = [
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];
/// Password managers mark what they copy so that clipboard tools leave it alone.
const PRIVATE: &str = "x-kde-passwordManagerHint";
/// How long another app may take to hand the copied text over, or to take a viewer's.
const TRANSFER_LIMIT: Duration = Duration::from_secs(2);
/// How long a viewer's paste waits for the desktop to take their text.
const PUT_LIMIT: Duration = Duration::from_millis(500);

/// The clipboard task of one portal session.
pub(super) struct Clipboard {
    puts: mpsc::UnboundedSender<(String, oneshot::Sender<()>)>,
    task: tokio::task::JoinHandle<()>,
}

impl Clipboard {
    pub(super) async fn start(
        desktop: &Desktop,
        events: mpsc::UnboundedSender<Event>,
    ) -> Option<Self> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender(DESKTOP)
            .path(DESKTOP_PATH)
            .interface(CLIPBOARD)
            .build()
            .ok()?;
        let signals = MessageStream::for_match_rule(rule, desktop.bus(), None)
            .await
            .ok()?;
        let (puts, put) = mpsc::unbounded_channel();
        let session = Session {
            bus: desktop.bus().clone(),
            session: OwnedObjectPath::from(desktop.session().to_owned()),
        };
        let task = tokio::spawn(run(session, signals, put, events));
        Some(Self { puts, task })
    }

    /// Puts a viewer's text on the computer's clipboard, and returns once the desktop has it, or
    /// after a moment.
    pub(super) async fn put(&self, text: String) {
        let (done, taken) = oneshot::channel();
        if self.puts.send((text, done)).is_ok() {
            drop(tokio::time::timeout(PUT_LIMIT, taken).await);
        }
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// What the clipboard task calls on.
struct Session {
    bus: zbus::Connection,
    session: OwnedObjectPath,
}

#[derive(Debug, Default, DeserializeDict, Type)]
#[zbus(signature = "a{sv}")]
struct Owner {
    mime_types: Option<Vec<String>>,
    session_is_owner: Option<bool>,
}

async fn run(
    session: Session,
    mut signals: MessageStream,
    mut puts: mpsc::UnboundedReceiver<(String, oneshot::Sender<()>)>,
    events: mpsc::UnboundedSender<Event>,
) {
    let mut offered: Option<String> = None;
    loop {
        tokio::select! {
            signal = signals.next() => {
                let Some(Ok(signal)) = signal else {
                    return;
                };
                if !session.heard(&signal, offered.as_deref(), &events).await {
                    return;
                }
            }
            put = puts.recv() => {
                let Some((text, done)) = put else {
                    return;
                };
                if session.offer().await {
                    offered = Some(text);
                }
                // The paste that waits for the text goes on.
                drop(done);
            }
        }
    }
}

impl Session {
    /// Returns false once the helper no longer listens.
    async fn heard(
        &self,
        signal: &zbus::Message,
        offered: Option<&str>,
        events: &mpsc::UnboundedSender<Event>,
    ) -> bool {
        let header = signal.header();
        match header.member().map(MemberName::as_str) {
            Some("SelectionOwnerChanged") => {
                let Ok((path, owner)) = signal.body().deserialize::<(OwnedObjectPath, Owner)>()
                else {
                    return true;
                };
                if path != self.session || owner.session_is_owner.unwrap_or_default() {
                    return true;
                }
                let kinds = owner.mime_types.unwrap_or_default();
                if kinds.iter().any(|kind| kind == PRIVATE) {
                    return true;
                }
                if let Some(kind) = TEXT
                    .into_iter()
                    .find(|text| kinds.iter().any(|kind| kind == text))
                    && let Some(text) = self.read(kind).await
                {
                    return events.send(Event::Copied(text)).is_ok();
                }
            }
            Some("SelectionTransfer") => {
                if let Ok((path, _, serial)) = signal
                    .body()
                    .deserialize::<(OwnedObjectPath, String, u32)>()
                    && path == self.session
                {
                    self.write(serial, offered).await;
                }
            }
            _ => {}
        }
        true
    }

    async fn offer(&self) -> bool {
        let options: HashMap<&str, Variant<'_>> =
            HashMap::from([("mime_types", Variant::from(TEXT.to_vec()))]);
        self.call("SetSelection", &(&self.session, options))
            .await
            .is_ok()
    }

    async fn read(&self, kind: &str) -> Option<String> {
        let reply = self
            .call("SelectionRead", &(&self.session, kind))
            .await
            .ok()?;
        let source: OwnedFd = reply.body().deserialize::<zbus::OwnedFd>().ok()?.into();
        let source = pipe::Receiver::from_owned_fd(source).ok()?;
        let mut bytes = Vec::new();
        let most = u64::try_from(MOST_CLIPBOARD).unwrap_or(u64::MAX) + 1;
        tokio::time::timeout(TRANSFER_LIMIT, source.take(most).read_to_end(&mut bytes))
            .await
            .ok()?
            .ok()?;
        let text = String::from_utf8(bytes).ok()?;
        (!text.is_empty() && text.len() <= MOST_CLIPBOARD).then_some(text)
    }

    /// Hands the offered text to the app that pastes it.
    async fn write(&self, serial: u32, text: Option<&str>) {
        let written = match text {
            Some(text) => self.send(serial, text).await.is_some(),
            None => false,
        };
        drop(
            self.call("SelectionWriteDone", &(&self.session, serial, written))
                .await,
        );
    }

    async fn send(&self, serial: u32, text: &str) -> Option<()> {
        let reply = self
            .call("SelectionWrite", &(&self.session, serial))
            .await
            .ok()?;
        let target: OwnedFd = reply.body().deserialize::<zbus::OwnedFd>().ok()?.into();
        let mut target = pipe::Sender::from_owned_fd(target).ok()?;
        tokio::time::timeout(TRANSFER_LIMIT, target.write_all(text.as_bytes()))
            .await
            .ok()?
            .ok()
    }

    async fn call<B>(&self, method: &'static str, body: &B) -> Result<zbus::Message, Problem>
    where
        B: serde::Serialize + DynamicType + Sync,
    {
        Ok(self
            .bus
            .call_method(Some(DESKTOP), DESKTOP_PATH, Some(CLIPBOARD), method, body)
            .await?)
    }
}
