use std::path::Path;

use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{
    DosiComputer, DosiPage, DosiSnapshot, DosiState,
    chat::{Account, Chat, Message, Output},
};
use crate::{Error, Result, agents::AgentChoice};

pub(crate) struct Viewer {
    chat: Chat,
}

impl Viewer {
    pub(crate) fn load(data_root: &Path) -> (Self, mpsc::Receiver<Message>) {
        let (chat, receiver) = Chat::load(data_root);
        (Self { chat }, receiver)
    }

    #[cfg_attr(
        feature = "host",
        expect(dead_code, reason = "a computer runs the engine")
    )]
    pub(crate) fn sender(&self) -> mpsc::Sender<Message> {
        self.chat.sender()
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.chat.generation()
    }

    pub(crate) fn share(&mut self, account: Option<(Uuid, String)>, out: &mut Output) {
        let account = account.map(|(user, device)| Account {
            user,
            device,
            computer: String::new(),
        });
        if self.chat.differs(account.as_ref()) {
            self.chat.share(account, out);
        }
    }

    pub(crate) fn nudge(&mut self, sequence: Option<u64>, out: &mut Output) {
        self.chat.nudge(sequence, out);
    }

    pub(crate) fn handle(&mut self, message: Message, out: &mut Output) {
        match message {
            Message::Written { generation, result } => self.chat.written(generation, result, out),
            Message::Synced { generation, result } => {
                self.chat.synced(generation, result, out);
            }
            Message::Retry { generation } => self.chat.retry(generation, out),
            Message::Gone { generation, turn } => {
                self.chat.gone(generation, &turn, out);
            }
            #[cfg(feature = "host")]
            Message::Saved { .. } | Message::Run(_) => {}
        }
    }

    pub(crate) fn snapshot(&self) -> Value {
        let state = match self.chat.remote() {
            Some(remote) if remote.stopping() => DosiState::Stopping,
            Some(_) => DosiState::Working,
            None if !self.chat.ready() => DosiState::Connecting,
            None => DosiState::Idle,
        };
        let (conversation, items, earlier) = self.chat.conversation_snapshot();
        let snapshot = DosiSnapshot {
            state,
            setup: AgentChoice::default(),
            agents: Vec::new(),
            instructions_edited: false,
            folder: String::new(),
            shared: self.chat.shared(),
            remote: self.chat.remote_snapshot(),
            conversation,
            items,
            earlier,
            queued: Vec::new(),
            asks: self.chat.waiting(),
            last: self.chat.last(),
        };
        json!({"type":"dosi.snapshot","dosi":snapshot})
    }

    pub(crate) fn send(
        &mut self,
        text: &str,
        context: Option<String>,
        computer: Option<DosiComputer>,
        out: &mut Output,
    ) -> Result<()> {
        let computer = computer
            .ok_or_else(|| Error::Refused(text!("Choose one of your computers for Dosi.")))?;
        self.chat.ask(text.trim(), context, computer, out)
    }

    pub(crate) fn stop(&mut self, out: &mut Output) {
        self.chat.stop_remote(out);
    }

    pub(crate) fn unqueue(&mut self, id: &str, out: &mut Output) -> Result<()> {
        if self.chat.take_back(id, out) {
            Ok(())
        } else {
            Err(Error::Refused(text!("You sent that message already.")))
        }
    }

    pub(crate) fn answer(
        &mut self,
        item: &str,
        choices: Vec<String>,
        out: &mut Output,
    ) -> Result<()> {
        self.chat.answer(item, choices, out).map(drop)
    }

    pub(crate) fn renew(&mut self, out: &mut Output) {
        self.chat.renew(out);
    }

    pub(crate) fn page(&self, before: &str) -> DosiPage {
        self.chat.page(before)
    }
}
