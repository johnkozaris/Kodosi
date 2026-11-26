use super::{
    ChatAsk, ChatBody, ChatChange, ChatCommand, ChatFacts, ChatItem, ChatNow, ChatPerson,
    ChatQueued, ChatSnapshot, ChatState, ChatTask, ChatTool, ChatUpdate, MOST_ASKS, MOST_COMMANDS,
    MOST_ITEMS, MOST_QUEUED, MOST_TARGET, MOST_TASKS, MOST_TEXT, ToolKind, ToolState,
};

#[derive(Default)]
pub(super) struct Chat {
    revision: u64,
    state: ChatState,
    now: Option<ChatNow>,
    facts: ChatFacts,
    items: Vec<ChatItem>,
    asks: Vec<ChatAsk>,
    queued: Vec<ChatQueued>,
    tasks: Vec<ChatTask>,
    commands: Vec<ChatCommand>,
    changes: Vec<ChatChange>,
    made: u64,
    /// Used to shorten paths outside the terminal's working directory with `~`.
    home: Option<String>,
}

impl Chat {
    pub(super) fn at_home(home: Option<String>) -> Self {
        Self {
            home,
            ..Self::default()
        }
    }

    pub(super) fn home(&self) -> Option<&str> {
        self.home.as_deref()
    }

    pub(super) fn snapshot(&self) -> ChatSnapshot {
        ChatSnapshot {
            revision: self.revision,
            state: self.state,
            now: self.now.clone(),
            facts: self.facts.clone(),
            items: self.items.clone(),
            asks: self.asks.clone(),
            queued: self.queued.clone(),
            tasks: self.tasks.clone(),
            commands: self.commands.clone(),
        }
    }

    pub(super) fn take(&mut self) -> Option<ChatUpdate> {
        if self.changes.is_empty() {
            return None;
        }
        self.revision += 1;
        Some(ChatUpdate {
            revision: self.revision,
            changes: std::mem::take(&mut self.changes),
        })
    }

    fn name(&mut self, letter: char) -> String {
        self.made += 1;
        format!("{letter}{}", self.made)
    }

    pub(super) fn push(&mut self, at: u64, body: ChatBody) -> String {
        let id = self.name('i');
        let item = ChatItem {
            id: id.clone(),
            at,
            body,
        };
        self.items.push(item.clone());
        self.changes.push(ChatChange::Item { item });
        while self.items.len() > MOST_ITEMS {
            let oldest = self.items.remove(0);
            self.forget(&oldest.id);
            self.changes.push(ChatChange::Remove { id: oldest.id });
        }
        id
    }

    fn forget(&mut self, id: &str) {
        self.changes.retain(|change| match change {
            ChatChange::Item { item } => item.id != id,
            ChatChange::Text { id: text, .. } => text != id,
            _ => true,
        });
    }

    pub(super) fn body(&self, id: &str) -> Option<&ChatBody> {
        self.items
            .iter()
            .rev()
            .find(|item| item.id == id)
            .map(|item| &item.body)
    }

    pub(super) fn tool(&self, id: &str) -> Option<&ChatTool> {
        match self.body(id)? {
            ChatBody::Tool { tool } => Some(tool),
            _ => None,
        }
    }

    pub(super) fn put(&mut self, id: &str, body: ChatBody) {
        let Some(item) = self.items.iter_mut().rev().find(|item| item.id == id) else {
            return;
        };
        if item.body == body {
            return;
        }
        item.body = body;
        let item = item.clone();
        self.changes
            .retain(|change| !matches!(change, ChatChange::Text { id: text, .. } if text == id));
        let earlier = self.changes.iter_mut().find(
            |change| matches!(change, ChatChange::Item { item: earlier } if earlier.id == id),
        );
        match earlier {
            Some(change) => *change = ChatChange::Item { item },
            None => self.changes.push(ChatChange::Item { item }),
        }
    }

    pub(super) fn change_tool(&mut self, id: &str, change: impl FnOnce(&mut ChatTool)) {
        let Some(mut tool) = self.tool(id).cloned() else {
            return;
        };
        change(&mut tool);
        self.put(
            id,
            ChatBody::Tool {
                tool: Box::new(tool),
            },
        );
    }

    pub(super) fn write(&mut self, id: &str, more: &str) {
        let Some(item) = self.items.iter_mut().rev().find(|item| item.id == id) else {
            return;
        };
        let ChatBody::Text { text, .. } = &mut item.body else {
            return;
        };
        let room = MOST_TEXT.saturating_sub(text.len());
        let more = start_of(more, room);
        if more.is_empty() {
            return;
        }
        text.push_str(more);
        let whole = self.changes.iter_mut().find_map(|change| match change {
            ChatChange::Item {
                item:
                    ChatItem {
                        id: earlier,
                        body: ChatBody::Text { text, .. },
                        ..
                    },
            } if earlier == id => Some(text),
            _ => None,
        });
        if let Some(text) = whole {
            text.push_str(more);
            return;
        }
        match self.changes.last_mut() {
            Some(ChatChange::Text { id: last, text }) if last == id => text.push_str(more),
            _ => self.changes.push(ChatChange::Text {
                id: id.to_owned(),
                text: more.to_owned(),
            }),
        }
    }

    pub(super) fn remove(&mut self, id: &str) {
        let before = self.items.len();
        self.items.retain(|item| item.id != id);
        if self.items.len() != before {
            self.forget(id);
            self.changes.push(ChatChange::Remove { id: id.to_owned() });
        }
    }

    pub(super) fn items(&self) -> &[ChatItem] {
        &self.items
    }

    /// The helpers of the agent that work now: the calls of helpers that run, also after the turn
    /// that started them.
    pub(super) fn helpers(&self) -> u8 {
        let working = self
            .items
            .iter()
            .filter(|item| {
                matches!(&item.body, ChatBody::Tool { tool }
                    if tool.kind == ToolKind::Agent && tool.state == ToolState::Running)
            })
            .count();
        u8::try_from(working).unwrap_or(u8::MAX)
    }

    pub(super) fn asks(&self) -> &[ChatAsk] {
        &self.asks
    }

    pub(super) fn ask(&mut self, ask: ChatAsk) {
        if self.asks.len() >= MOST_ASKS {
            self.asks.remove(0);
        }
        self.asks.push(ask);
        self.tell_asks();
    }

    pub(super) fn close_asks(&mut self, over: impl Fn(&ChatAsk) -> bool) -> Vec<String> {
        let closed = self
            .asks
            .iter()
            .filter(|ask| over(ask))
            .map(|ask| ask.id.clone())
            .collect::<Vec<_>>();
        if !closed.is_empty() {
            self.asks.retain(|ask| !closed.contains(&ask.id));
            self.tell_asks();
        }
        closed
    }

    fn tell_asks(&mut self) {
        self.changes
            .retain(|change| !matches!(change, ChatChange::Asks { .. }));
        self.changes.push(ChatChange::Asks {
            asks: self.asks.clone(),
        });
    }

    pub(super) fn queued(&self) -> &[ChatQueued] {
        &self.queued
    }

    pub(super) fn queue(&mut self, text: String, by: Option<ChatPerson>) -> Option<String> {
        if self.queued.len() >= MOST_QUEUED {
            return None;
        }
        let id = self.name('q');
        self.queued.push(ChatQueued {
            id: id.clone(),
            text,
            by,
            refused: None,
            terminal: false,
            given: false,
        });
        self.tell_queued();
        Some(id)
    }

    pub(super) fn unqueue(&mut self, id: &str) -> Option<ChatQueued> {
        let index = self.queued.iter().position(|queued| queued.id == id)?;
        let queued = self.queued.remove(index);
        self.tell_queued();
        Some(queued)
    }

    pub(super) fn refuse(&mut self, id: &str, reason: String) {
        self.change_queued(id, |queued| queued.refused = Some(reason));
    }

    pub(super) fn in_terminal(&mut self, id: &str) {
        self.change_queued(id, |queued| queued.terminal = true);
    }

    pub(super) fn give(&mut self, id: &str) {
        self.change_queued(id, |queued| queued.given = true);
    }

    pub(super) fn take_back(&mut self, id: &str) {
        self.change_queued(id, |queued| queued.given = false);
    }

    fn change_queued(&mut self, id: &str, change: impl FnOnce(&mut ChatQueued)) {
        if let Some(queued) = self.queued.iter_mut().find(|queued| queued.id == id) {
            change(queued);
            self.tell_queued();
        }
    }

    fn tell_queued(&mut self) {
        self.changes
            .retain(|change| !matches!(change, ChatChange::Queued { .. }));
        self.changes.push(ChatChange::Queued {
            queued: self.queued.clone(),
        });
    }

    pub(super) fn set_tasks(&mut self, tasks: impl IntoIterator<Item = ChatTask>) {
        let tasks = tasks
            .into_iter()
            .take(MOST_TASKS)
            .map(|task| ChatTask {
                text: start_of(&task.text, MOST_TARGET).to_owned(),
                ..task
            })
            .collect::<Vec<_>>();
        if tasks == self.tasks {
            return;
        }
        self.tasks = tasks;
        self.changes
            .retain(|change| !matches!(change, ChatChange::Tasks { .. }));
        self.changes.push(ChatChange::Tasks {
            tasks: self.tasks.clone(),
        });
    }

    pub(super) fn set_commands(&mut self, commands: impl IntoIterator<Item = ChatCommand>) {
        let commands = commands.into_iter().take(MOST_COMMANDS).collect::<Vec<_>>();
        if commands == self.commands {
            return;
        }
        self.commands = commands;
        self.changes
            .retain(|change| !matches!(change, ChatChange::Commands { .. }));
        self.changes.push(ChatChange::Commands {
            commands: self.commands.clone(),
        });
    }

    pub(super) fn facts(&self) -> &ChatFacts {
        &self.facts
    }

    pub(super) fn change_facts(&mut self, change: impl FnOnce(&mut ChatFacts)) {
        let mut facts = self.facts.clone();
        change(&mut facts);
        if facts == self.facts {
            return;
        }
        self.facts = facts;
        self.changes
            .retain(|change| !matches!(change, ChatChange::Facts { .. }));
        self.changes.push(ChatChange::Facts {
            facts: Box::new(self.facts.clone()),
        });
    }

    pub(super) fn set_state(&mut self, state: ChatState, now: Option<ChatNow>) {
        if state == self.state && now == self.now {
            return;
        }
        self.state = state;
        self.now.clone_from(&now);
        self.changes
            .retain(|change| !matches!(change, ChatChange::State { .. }));
        self.changes.push(ChatChange::State { state, now });
    }

    /// A new conversation: the items, the questions and the agent's list of its work go, and what
    /// the person queued and the agent's commands stay.
    pub(super) fn reset(&mut self) {
        self.items.clear();
        self.asks.clear();
        self.tasks.clear();
        self.changes.retain(|change| {
            matches!(
                change,
                ChatChange::Facts { .. }
                    | ChatChange::Queued { .. }
                    | ChatChange::State { .. }
                    | ChatChange::Commands { .. }
            )
        });
        self.changes.insert(0, ChatChange::Reset);
    }
}

pub(super) fn start_of(text: &str, most: usize) -> &str {
    &text[..text.floor_char_boundary(most)]
}

pub(super) fn ends_of(text: &str, most: usize) -> String {
    if text.len() <= most {
        return text.to_owned();
    }
    let head = start_of(text, most * 3 / 5);
    let tail = text.ceil_char_boundary(text.len() - most * 2 / 5);
    format!("{head}\n…\n{}", &text[tail..])
}
