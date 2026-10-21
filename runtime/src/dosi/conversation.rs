use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Instant,
};

use uuid::Uuid;

use super::{
    DosiBody, DosiItem, DosiPage, DosiTone, DosiToolStatus,
    adapters::Event,
    receipt::{self, Shown},
};

pub(super) const MAX_ITEMS: usize = 1000;
const MAX_TEXT_BYTES: usize = 256 * 1024;
const MAX_OUTPUT_BYTES: usize = 4 * 1024;
const WINDOW_ITEMS: usize = 60;
const WINDOW_BYTES: usize = 256 * 1024;
const PAGE_ITEMS: usize = 40;

struct Entry {
    item: DosiItem,
    sequence: Option<u64>,
    mine: bool,
}

pub(super) struct Conversation {
    pub id: String,
    pub started_at: i64,
    pub confirmed: bool,
    entries: VecDeque<Entry>,
    turn: Option<String>,
    blocks: HashMap<String, String>,
    tools: HashMap<String, String>,
    hidden: HashSet<String>,
    thinking: HashMap<String, Instant>,
}

pub(super) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

pub(super) fn time_of(id: &str) -> Option<i64> {
    let (seconds, nanos) = Uuid::parse_str(id).ok()?.get_timestamp()?.to_unix();
    i64::try_from(seconds)
        .ok()?
        .checked_mul(1000)?
        .checked_add(i64::from(nanos / 1_000_000))
}

pub(super) fn clip(text: &str, bytes: usize) -> String {
    if text.len() <= bytes {
        return text.to_owned();
    }
    let mut end = bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

impl Conversation {
    pub(super) fn begun(id: String) -> Self {
        Self {
            confirmed: true,
            ..Self::new(id, now())
        }
    }

    pub(super) fn new(id: String, started_at: i64) -> Self {
        Self {
            id,
            started_at,
            confirmed: false,
            entries: VecDeque::new(),
            turn: None,
            blocks: HashMap::new(),
            tools: HashMap::new(),
            hidden: HashSet::new(),
            thinking: HashMap::new(),
        }
    }

    pub(super) fn items(&self) -> impl DoubleEndedIterator<Item = &DosiItem> {
        self.entries.iter().map(|entry| &entry.item)
    }

    pub(super) fn item(&self, id: &str) -> Option<&DosiItem> {
        self.items().find(|item| item.id == id)
    }

    pub(super) fn latest(&self) -> u64 {
        self.entries
            .iter()
            .filter_map(|entry| entry.sequence)
            .max()
            .unwrap_or_default()
    }

    pub(super) fn turn(&self) -> Option<&str> {
        self.turn.as_deref()
    }

    pub(super) fn begin(&mut self, turn: String, person: Option<&str>) -> Option<DosiItem> {
        self.turn = Some(turn);
        self.blocks.clear();
        self.tools.clear();
        self.hidden.clear();
        self.thinking.clear();
        person.map(|text| {
            self.push(DosiBody::Person {
                text: text.to_owned(),
            })
        })
    }

    pub(super) fn end(&mut self) {
        self.turn = None;
    }

    pub(super) fn push(&mut self, body: DosiBody) -> DosiItem {
        let item = DosiItem {
            id: Uuid::now_v7().to_string(),
            turn: self
                .turn
                .clone()
                .unwrap_or_else(|| Uuid::now_v7().to_string()),
            at: now(),
            body,
        };
        self.entries.push_back(Entry {
            item: item.clone(),
            sequence: None,
            mine: true,
        });
        self.bound();
        item
    }

    pub(super) fn upsert(
        &mut self,
        item: DosiItem,
        sequence: u64,
        mine: bool,
        restoring: bool,
    ) -> bool {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.item.id == item.id)
        {
            entry.sequence = Some(entry.sequence.map_or(sequence, |known| known.max(sequence)));
            if (mine && !restoring) || entry.item == item {
                return false;
            }
            entry.item = item;
            entry.mine &= mine;
            return true;
        }
        let position = self
            .entries
            .iter()
            .position(|entry| entry.sequence.is_none_or(|known| known > sequence))
            .unwrap_or(self.entries.len());
        self.entries.insert(
            position,
            Entry {
                item,
                sequence: Some(sequence),
                mine,
            },
        );
        self.bound();
        true
    }

    fn bound(&mut self) {
        while self.entries.len() > MAX_ITEMS {
            self.entries.pop_front();
        }
    }

    pub(super) fn foreign_since(&self, seen: u64) -> Vec<&DosiItem> {
        self.entries
            .iter()
            .filter(|entry| !entry.mine && entry.sequence.is_some_and(|sequence| sequence > seen))
            .map(|entry| &entry.item)
            .collect()
    }

    fn find(&mut self, id: &str) -> Option<&mut DosiItem> {
        self.entries
            .iter_mut()
            .rev()
            .map(|entry| &mut entry.item)
            .find(|item| item.id == id)
    }

    fn block(&mut self, block: &str, body: impl FnOnce() -> DosiBody) -> &mut DosiItem {
        let known = self
            .blocks
            .get(block)
            .and_then(|id| self.entries.iter().rposition(|entry| &entry.item.id == id));
        let position = known.unwrap_or_else(|| {
            let id = self.push(body()).id;
            self.blocks.insert(block.to_owned(), id);
            self.entries.len() - 1
        });
        &mut self.entries[position].item
    }

    pub(super) fn apply(&mut self, event: Event) -> Option<DosiItem> {
        match event {
            Event::TextDelta { block, text } => Some(self.stream(&block, &text, false)),
            Event::ReasoningDelta { block, text } => Some(self.stream(&block, &text, true)),
            Event::TextDone { block, text } => self.text_done(&block, &text),
            Event::ReasoningDone { block, text } => Some(self.reasoning_done(&block, &text)),
            Event::ToolStart { id, name, input } => self.tool_start(id, &name, &input),
            Event::ToolEnd { id, failed, output } => self.tool_end(&id, failed, &output),
            Event::Session(_) | Event::TurnEnd { .. } | Event::Reply(_) => None,
        }
    }

    fn stream(&mut self, block: &str, text: &str, reasoning: bool) -> DosiItem {
        if reasoning {
            self.thinking
                .entry(block.to_owned())
                .or_insert_with(Instant::now);
        }
        let item = self.block(block, || {
            if reasoning {
                DosiBody::Reasoning {
                    text: String::new(),
                    live: true,
                    seconds: None,
                }
            } else {
                DosiBody::Text {
                    text: String::new(),
                    live: true,
                }
            }
        });
        if let DosiBody::Text { text: current, .. } | DosiBody::Reasoning { text: current, .. } =
            &mut item.body
            && current.len() < MAX_TEXT_BYTES
        {
            current.push_str(text);
        }
        item.clone()
    }

    fn text_done(&mut self, block: &str, text: &str) -> Option<DosiItem> {
        if text.trim().is_empty() && !self.blocks.contains_key(block) {
            return None;
        }
        let item = self.block(block, || DosiBody::Text {
            text: String::new(),
            live: false,
        });
        item.body = DosiBody::Text {
            text: clip(text, MAX_TEXT_BYTES),
            live: false,
        };
        Some(item.clone())
    }

    fn reasoning_done(&mut self, block: &str, text: &str) -> DosiItem {
        let seconds = self
            .thinking
            .remove(block)
            .map(|start| u32::try_from(start.elapsed().as_secs()).unwrap_or(u32::MAX));
        let item = self.block(block, || DosiBody::Reasoning {
            text: String::new(),
            live: false,
            seconds: None,
        });
        let text = match &item.body {
            DosiBody::Reasoning { text: streamed, .. } if text.is_empty() => streamed.clone(),
            _ => clip(text, MAX_TEXT_BYTES),
        };
        item.body = DosiBody::Reasoning {
            text,
            live: false,
            seconds,
        };
        item.clone()
    }

    fn tool_start(
        &mut self,
        id: String,
        name: &str,
        input: &serde_json::Value,
    ) -> Option<DosiItem> {
        let Shown::Receipt(receipt) = receipt::of(name, input) else {
            self.hidden.insert(id);
            return None;
        };
        let item = self.push(DosiBody::Tool {
            action: receipt.action,
            target: receipt.target,
            text: receipt.text,
            status: DosiToolStatus::Running,
            output: None,
        });
        self.tools.insert(id, item.id.clone());
        Some(item)
    }

    fn tool_end(&mut self, id: &str, failed: bool, output: &str) -> Option<DosiItem> {
        if self.hidden.contains(id) {
            return None;
        }
        let item_id = self.tools.get(id)?.clone();
        let item = self.find(&item_id)?;
        if let DosiBody::Tool {
            status,
            output: shown,
            ..
        } = &mut item.body
        {
            *status = if failed {
                DosiToolStatus::Failed
            } else {
                DosiToolStatus::Done
            };
            let output = output.trim();
            *shown = (!output.is_empty()).then(|| clip(output, MAX_OUTPUT_BYTES));
        }
        Some(item.clone())
    }

    pub(super) fn settle(&mut self, interrupted: bool) -> Vec<DosiItem> {
        let Some(turn) = self.turn.clone() else {
            return Vec::new();
        };
        let mut changed = Vec::new();
        for entry in self
            .entries
            .iter_mut()
            .filter(|entry| entry.mine && entry.item.turn == turn)
        {
            let body = finished(entry.item.body.clone(), interrupted);
            if body != entry.item.body {
                entry.item.body = body;
                changed.push(entry.item.clone());
            }
        }
        changed
    }

    pub(super) fn answer(
        &mut self,
        id: &str,
        choices: Vec<String>,
    ) -> Result<DosiItem, &'static str> {
        let closed = "This question is closed.";
        let item = self.find(id).ok_or(closed)?;
        let DosiBody::Question {
            answer,
            options,
            multiple,
            ..
        } = &mut item.body
        else {
            return Err(closed);
        };
        if answer.is_some() {
            return Err(closed);
        }
        let own = choices
            .iter()
            .filter(|choice| !options.contains(choice))
            .count();
        let distinct = choices.iter().collect::<HashSet<_>>().len() == choices.len();
        if choices.is_empty() || own > 1 || !distinct || (!*multiple && choices.len() > 1) {
            return Err("Choose from the answers, and add at most one of your own.");
        }
        *answer = Some(choices);
        Ok(item.clone())
    }

    pub(super) fn close_questions(&mut self) -> Vec<DosiItem> {
        let mut changed = Vec::new();
        for entry in &mut self.entries {
            if let DosiBody::Question { answer, .. } = &mut entry.item.body
                && answer.is_none()
            {
                *answer = Some(Vec::new());
                changed.push(entry.item.clone());
            }
        }
        changed
    }

    pub(super) fn window(&self) -> (Vec<DosiItem>, bool) {
        let mut bytes = 0;
        let mut window = Vec::new();
        for item in self.items().rev() {
            let size = size(item);
            if window.len() == WINDOW_ITEMS || (!window.is_empty() && bytes + size > WINDOW_BYTES) {
                break;
            }
            bytes += size;
            window.push(item.clone());
        }
        let earlier = window.len() < self.entries.len();
        window.reverse();
        (window, earlier)
    }

    pub(super) fn page(&self, before: &str) -> DosiPage {
        let end = self
            .entries
            .iter()
            .position(|entry| entry.item.id == before)
            .unwrap_or(0);
        let start = end.saturating_sub(PAGE_ITEMS);
        DosiPage {
            items: self
                .entries
                .range(start..end)
                .map(|entry| entry.item.clone())
                .collect(),
            earlier: start > 0,
        }
    }
}

fn size(item: &DosiItem) -> usize {
    match &item.body {
        DosiBody::Person { text }
        | DosiBody::Text { text, .. }
        | DosiBody::Reasoning { text, .. }
        | DosiBody::Note { text } => text.len(),
        DosiBody::Tool { text, output, .. } => {
            text.as_ref().map_or(0, String::len) + output.as_ref().map_or(0, String::len)
        }
        DosiBody::Question {
            question,
            options,
            keys,
            answer,
            ..
        } => std::iter::once(question)
            .chain(options)
            .chain(keys)
            .chain(answer.iter().flatten())
            .map(String::len)
            .sum(),
        DosiBody::Notice { text, .. } => text.as_ref().map_or(0, String::len),
    }
    .saturating_add(128)
}

pub(super) fn finished(body: DosiBody, interrupted: bool) -> DosiBody {
    match body {
        DosiBody::Text { text, .. } => DosiBody::Text { text, live: false },
        DosiBody::Reasoning { text, seconds, .. } => DosiBody::Reasoning {
            text,
            live: false,
            seconds,
        },
        DosiBody::Tool {
            action,
            target,
            text,
            status: DosiToolStatus::Running,
            output,
        } => DosiBody::Tool {
            action,
            target,
            text,
            status: if interrupted {
                DosiToolStatus::Failed
            } else {
                DosiToolStatus::Done
            },
            output,
        },
        other => other,
    }
}

pub(super) fn tone(message: &str) -> DosiTone {
    let lower = message.to_ascii_lowercase();
    if [
        "/login",
        "log in",
        "login",
        "sign in",
        "authenticat",
        "api key",
        "401",
        "unauthorized",
    ]
    .iter()
    .any(|word| lower.contains(word))
    {
        DosiTone::SignIn
    } else {
        DosiTone::Failed
    }
}
