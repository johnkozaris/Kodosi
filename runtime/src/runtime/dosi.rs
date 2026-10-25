use bytes::Bytes;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Completion, Runtime};
use crate::{
    Command, Error, Result,
    dosi::{DosiContext, DosiRecord, Failure, Message, Output, SharedInstructions},
    network,
};

fn failure(error: &network::Error) -> Failure {
    Failure {
        retry: !matches!(
            error,
            network::Error::Backend {
                status: 400 | 413,
                ..
            }
        ),
        message: error.to_string(),
    }
}

impl Runtime {
    pub(super) fn dosi_command(&mut self, command: &Command) -> Result<()> {
        let mut out = Output::default();
        let extra = match command {
            Command::DosiSend { text, context, .. } => {
                let context = context
                    .as_ref()
                    .and_then(|context| self.dosi_context(context));
                self.dosi.send(text, context, &mut out)?;
                None
            }
            Command::DosiStop { .. } => {
                self.dosi.stop(&mut out);
                None
            }
            Command::DosiUnqueue { queued_id, .. } => {
                self.dosi.unqueue(queued_id, &mut out)?;
                None
            }
            Command::DosiAnswer {
                item_id, choices, ..
            } => {
                self.dosi.answer(item_id, choices.clone(), &mut out)?;
                None
            }
            Command::DosiAsk {
                token,
                question,
                options,
                keys,
                multiple,
                terminal,
                ..
            } => {
                self.dosi.ask(
                    token,
                    question.clone(),
                    options.clone(),
                    keys.clone(),
                    *multiple,
                    terminal.clone(),
                    &mut out,
                )?;
                None
            }
            Command::DosiNew { .. } => {
                self.dosi.renew(&mut out);
                None
            }
            Command::DosiConfigure { change, .. } => {
                self.dosi.configure(change, &mut out)?;
                None
            }
            Command::DosiReadInstructions { .. } => {
                Some(json!({"instructions":self.dosi.read_instructions()}))
            }
            Command::DosiWriteInstructions { text, .. } => {
                let instructions = self.dosi.write_instructions(text.as_deref(), &mut out)?;
                Some(json!({"instructions":instructions}))
            }
            Command::DosiHistory { before, .. } => Some(json!({"page":self.dosi.page(before)})),
            _ => return Err(Error::Invalid("expected a Dosi command".to_owned())),
        };
        self.dosi_output(out);
        let mut answer = super::answer(command, Ok(()));
        if let Some(Value::Object(fields)) = extra {
            for (key, value) in fields {
                answer[key] = value;
            }
        }
        self.emit(answer);
        Ok(())
    }

    pub(super) fn learn_agents(&mut self, command: &Command, value: &Value) {
        if !matches!(command, Command::InspectAgents { .. }) {
            return;
        }
        if let Ok(catalog) = serde_json::from_value(value.clone()) {
            let mut out = Output::default();
            self.dosi.learn(&catalog, &mut out);
            self.dosi_output(out);
        }
    }

    pub(super) fn dosi_account(&mut self) {
        let user = self
            .network
            .identity()
            .filter(|identity| identity.enrolled)
            .and_then(|identity| Uuid::parse_str(&identity.user_id).ok());
        let mut out = Output::default();
        self.dosi.share(user, network::host_label, &mut out);
        self.dosi_output(out);
    }

    pub(super) fn dosi_nudge(&mut self, event: &Value) {
        let mut out = Output::default();
        self.dosi
            .nudge(event.get("sequence").and_then(Value::as_u64), &mut out);
        self.dosi_output(out);
    }

    pub(super) fn dosi_output(&mut self, out: Output) {
        for event in out.events {
            self.emit(event);
        }
        if out.snapshot {
            self.emit(self.dosi.snapshot());
        }
        for typing in out.typing {
            self.type_for_dosi(&typing.terminal, typing.text);
        }
        if let Some(record) = out.write {
            self.dosi_write(record);
        }
        if let Some(after) = out.sync {
            self.dosi_sync(after);
        }
        if let Some(instructions) = out.save {
            self.dosi_save(instructions);
        }
    }

    fn dosi_write(&self, record: DosiRecord) {
        let (network, messages, generation) = (
            self.network.clone(),
            self.dosi.sender(),
            self.dosi.generation(),
        );
        tokio::spawn(async move {
            let result = network
                .dosi_write(&record)
                .await
                .map_err(|error| failure(&error));
            drop(messages.send(Message::Written { generation, result }).await);
        });
    }

    fn dosi_sync(&self, after: u64) {
        let (network, messages, generation) = (
            self.network.clone(),
            self.dosi.sender(),
            self.dosi.generation(),
        );
        tokio::spawn(async move {
            let result = network
                .dosi_read(after)
                .await
                .map_err(|error| failure(&error));
            drop(messages.send(Message::Synced { generation, result }).await);
        });
    }

    fn dosi_save(&self, instructions: SharedInstructions) {
        let (network, messages, generation) = (
            self.network.clone(),
            self.dosi.sender(),
            self.dosi.generation(),
        );
        tokio::spawn(async move {
            let result = network
                .dosi_save(instructions.clone())
                .await
                .map_err(|error| failure(&error));
            let saved = Message::Saved {
                generation,
                instructions,
                result,
            };
            drop(messages.send(saved).await);
        });
    }

    fn type_for_dosi(&mut self, reference: &str, text: String) {
        let input = self.job_capacity().and_then(|()| {
            let id = self.find_terminal(reference).ok_or(Error::NotFound)?;
            Ok(self.target(id)?.send_input(Bytes::from(text.into_bytes())))
        });
        match input {
            Ok(input) => self.spawn(async move {
                Completion::Input {
                    result: input.await,
                }
            }),
            Err(error) => self.emit(json!({"type":"system.error","message":format!("Kodosi did not type the answer in {reference}: {error}")})),
        }
    }

    fn find_terminal(&self, reference: &str) -> Option<Uuid> {
        let entries = self.entries();
        let named = entries
            .iter()
            .map(|entry| (entry.id.as_str(), entry.name.as_str()));
        match crate::protocol::referred(reference, named)[..] {
            [id] => Uuid::parse_str(id).ok(),
            _ => None,
        }
    }

    fn dosi_context(&self, context: &DosiContext) -> Option<String> {
        let terminal = context.terminal.as_ref().and_then(|id| {
            self.entries()
                .into_iter()
                .find(|entry| &entry.id == id)
                .map(|entry| {
                    format!(
                        "the terminal \"{}\" ({})",
                        entry.name,
                        crate::protocol::short_id(&entry.id)
                    )
                })
        });
        let room = context.room.as_ref().and_then(|id| {
            self.cached_events
                .get("missions.snapshot")?
                .get("missions")?
                .as_array()?
                .iter()
                .find(|mission| mission.get("id").and_then(Value::as_str) == Some(id))?
                .get("name")?
                .as_str()
                .map(|name| format!("the room \"{name}\""))
        });
        match (terminal, room) {
            (Some(terminal), Some(room)) => Some(format!(
                "(In Kodosi the person has {terminal} open, in {room}.)"
            )),
            (Some(open), None) | (None, Some(open)) => {
                Some(format!("(In Kodosi the person has {open} open.)"))
            }
            (None, None) => None,
        }
    }
}
