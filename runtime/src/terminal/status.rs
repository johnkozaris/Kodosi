use serde::{Deserialize, Serialize};

const MAX_RECORDS: usize = 64;
const MAX_APP_BYTES: usize = 32;
const MAX_TITLE_CHARS: usize = 192;
const MAX_MESSAGE_CHARS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ProgramState {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
}

impl ProgramState {
    const fn urgency(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Done => 1,
            Self::Working => 2,
            Self::Error => 3,
            Self::Blocked => 4,
        }
    }

    const fn ends_with_program(self) -> bool {
        matches!(self, Self::Idle | Self::Working | Self::Blocked)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum BlockedKind {
    Permission,
    Question,
    Auth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgramStatus {
    pub state: ProgramState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<BlockedKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ProgramStatus {
    pub fn is_valid(&self) -> bool {
        (self.kind.is_none() || self.state == ProgramState::Blocked)
            && self.progress.is_none_or(|progress| {
                progress <= 100
                    && matches!(self.state, ProgramState::Working | ProgramState::Blocked)
            })
            && self.app.as_deref().is_none_or(is_app_name)
            && self
                .title
                .as_deref()
                .is_none_or(|title| is_display_text(title, MAX_TITLE_CHARS))
            && self
                .message
                .as_deref()
                .is_none_or(|message| is_display_text(message, MAX_MESSAGE_CHARS))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProgramStatusReport {
    pub id: String,
    pub state: Option<ProgramState>,
    pub kind: Option<BlockedKind>,
    pub progress: Option<u8>,
    pub app: Option<String>,
    pub title: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug)]
struct Record {
    id: String,
    state: ProgramState,
    kind: Option<BlockedKind>,
    progress: Option<u8>,
    app: Option<String>,
    title: Option<String>,
    message: Option<String>,
    updated: u64,
}

#[derive(Debug, Default)]
pub(super) struct ProgramStatusRecords {
    records: Vec<Record>,
    updates: u64,
    reported: bool,
}

impl ProgramStatusRecords {
    pub(super) fn apply(&mut self, report: ProgramStatusReport) {
        self.reported = true;
        self.replace(report);
    }

    pub(super) fn apply_progress(&mut self, report: ProgramStatusReport) {
        if !self.reported {
            self.replace(report);
        }
    }

    pub(super) fn reset(&mut self) {
        self.records.clear();
        self.reported = false;
    }

    fn replace(&mut self, report: ProgramStatusReport) {
        let Some(state) = report.state else {
            self.clear(&report.id);
            return;
        };
        self.updates += 1;
        let record = Record {
            state,
            kind: report.kind.filter(|_| state == ProgramState::Blocked),
            progress: report.progress.filter(|progress| {
                *progress <= 100 && matches!(state, ProgramState::Working | ProgramState::Blocked)
            }),
            app: report.app.filter(|app| is_app_name(app)),
            title: report
                .title
                .and_then(|title| display_text(&title, MAX_TITLE_CHARS)),
            message: report
                .message
                .and_then(|message| display_text(&message, MAX_MESSAGE_CHARS)),
            updated: self.updates,
            id: report.id,
        };
        if let Some(existing) = self
            .records
            .iter_mut()
            .find(|existing| existing.id == record.id)
        {
            *existing = record;
            return;
        }
        if self.records.len() == MAX_RECORDS
            && let Some(oldest) = self
                .records
                .iter()
                .enumerate()
                .min_by_key(|(_, record)| record.updated)
                .map(|(index, _)| index)
        {
            self.records.swap_remove(oldest);
        }
        self.records.push(record);
    }

    pub(super) fn program_exited(&mut self) {
        self.records
            .retain(|record| !record.state.ends_with_program());
        self.reported = false;
    }

    pub(super) fn summary(&self) -> Option<ProgramStatus> {
        let record = self
            .records
            .iter()
            .max_by_key(|record| (record.state.urgency(), record.updated))?;
        Some(ProgramStatus {
            state: record.state,
            kind: record.kind,
            progress: record.progress,
            app: self.app(record).map(str::to_owned),
            title: record.title.clone(),
            message: record.message.clone(),
        })
    }

    fn clear(&mut self, id: &str) {
        if id.is_empty() {
            self.records.clear();
        } else {
            self.records
                .retain(|record| record.id != id && !is_descendant(&record.id, id));
        }
    }

    fn app<'a>(&'a self, record: &'a Record) -> Option<&'a str> {
        record.app.as_deref().or_else(|| {
            self.records
                .iter()
                .filter(|ancestor| {
                    ancestor.app.is_some()
                        && (ancestor.id.is_empty() || is_descendant(&record.id, &ancestor.id))
                        && ancestor.id != record.id
                })
                .max_by_key(|ancestor| ancestor.id.len())
                .and_then(|ancestor| ancestor.app.as_deref())
        })
    }
}

fn is_descendant(id: &str, ancestor: &str) -> bool {
    id.strip_prefix(ancestor)
        .is_some_and(|rest| rest.starts_with('/'))
}

fn is_app_name(app: &str) -> bool {
    (1..=MAX_APP_BYTES).contains(&app.len())
        && app
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'+' | b'-'))
}

const fn is_invisible_format(character: char) -> bool {
    matches!(
        character,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{2069}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

fn is_display_text(text: &str, max_chars: usize) -> bool {
    !text.is_empty()
        && text.chars().count() <= max_chars
        && !text
            .chars()
            .any(|character| character.is_control() || is_invisible_format(character))
}

fn display_text(text: &str, max_chars: usize) -> Option<String> {
    let text = text
        .chars()
        .filter(|character| !character.is_control() && !is_invisible_format(*character))
        .take(max_chars)
        .collect::<String>();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
