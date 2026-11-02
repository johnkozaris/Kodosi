//! The agent's commands. An agent is one more client of the contract: a command joins a live
//! document as a viewer in the agent's name, and its edits show at once, as a person's do. A
//! host keeps the agent's seat for a short time after the last command, so the agent stays among
//! the people who are here.

use std::{
    collections::HashMap,
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use clap::{Args, Subcommand};
use document::{CONTRACT, Catchup, Format, Frame, Guide, Peer, ToClient, ToDocument, contract};
use documents::{Documents, Formats, OpenError, Pipe, Viewer, format};
use excel::Excel;
use pdf::Pdf;
use powerpoint::PowerPoint;
use serde_json::{Value, json};
use word::Word;

const ANSWER_LIMIT: Duration = Duration::from_mins(1);
/// How long an agent stays among the people after its last command.
const SEAT_KEPT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum Failed {
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Says(String),
    #[error("{0}")]
    Refused(String),
}

/// One command of an agent.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Makes a Word, Excel or `PowerPoint` file.
    New(NewArguments),
    /// Prints the content with its ids and cell names, and the rev.
    Read(ReadArguments),
    /// Sends named ops as one change, all or none.
    Apply(ApplyArguments),
    /// Lists formula errors, shapes that leave the slide and other problems.
    Check(FileArguments),
    /// Says who is here, and what each person has selected.
    Selection(FileArguments),
    /// Prints each op of a format with its fields.
    Guide(GuideArguments),
}

#[derive(Debug, Clone, Args)]
pub struct NewArguments {
    pub file: PathBuf,
    /// A Markdown, CSV or JSON file to start from.
    #[arg(long)]
    pub from: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct ReadArguments {
    pub file: PathBuf,
    /// A block id, a cell range such as Sheet1!A1:C9, a slide id or a page number.
    #[arg(long = "where")]
    pub place: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct ApplyArguments {
    pub file: PathBuf,
    /// The ops as JSON, or the path of a JSON file.
    #[arg(long)]
    pub ops: String,
    /// The rev that the last read printed.
    #[arg(long)]
    pub base: u64,
    /// Writes the ops as tracked changes in Word.
    #[arg(long)]
    pub track: bool,
}

#[derive(Debug, Clone, Args)]
pub struct FileArguments {
    pub file: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct GuideArguments {
    /// word, excel, powerpoint or pdf.
    pub format: String,
    /// One op of the format.
    pub op: Option<String>,
}

impl Command {
    /// Takes relative paths from the folder of the agent.
    #[must_use]
    pub fn rooted(self, folder: &Path) -> Self {
        match self {
            Self::New(arguments) => Self::New(NewArguments {
                file: folder.join(arguments.file),
                from: arguments.from.map(|from| folder.join(from)),
            }),
            Self::Read(arguments) => Self::Read(ReadArguments {
                file: folder.join(arguments.file),
                ..arguments
            }),
            Self::Apply(arguments) => Self::Apply(ApplyArguments {
                file: folder.join(arguments.file),
                ops: if arguments.ops.trim_start().starts_with(['[', '{']) {
                    arguments.ops
                } else {
                    folder.join(arguments.ops).display().to_string()
                },
                ..arguments
            }),
            Self::Check(arguments) => Self::Check(FileArguments {
                file: folder.join(arguments.file),
            }),
            Self::Selection(arguments) => Self::Selection(FileArguments {
                file: folder.join(arguments.file),
            }),
            Self::Guide(arguments) => Self::Guide(arguments),
        }
    }
}

/// All four formats, by file extension.
pub fn formats() -> Formats {
    Formats::default()
        .with("docx", format::<Word>())
        .with("xlsx", format::<Excel>())
        .with("pptx", format::<PowerPoint>())
        .with("pdf", format::<Pdf>())
}

fn kind_of(path: &Path) -> Result<&'static str, Failed> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "docx" => Ok("word"),
        "xlsx" => Ok("excel"),
        "pptx" => Ok("powerpoint"),
        "pdf" => Ok("pdf"),
        _ => Err(Failed::Says(
            "This command takes .docx, .xlsx, .pptx and .pdf files.".to_owned(),
        )),
    }
}

fn read_query(kind: &str, place: Option<&str>) -> Value {
    match (kind, place) {
        ("word", place) => json!({ "kind": "read", "block": place }),
        ("excel", Some(place)) => match place.split_once('!') {
            Some((sheet, range)) => json!({ "kind": "read", "sheet": sheet, "range": range }),
            None if place.contains(':') || is_cell(place) => {
                json!({ "kind": "read", "range": place })
            }
            None => json!({ "kind": "read", "sheet": place }),
        },
        ("excel", None) => json!({ "kind": "read" }),
        ("powerpoint", place) => json!({ "kind": "read", "slide": place }),
        (_, place) => json!({
            "kind": "read",
            "page": place.and_then(|page| page.parse::<u32>().ok()),
        }),
    }
}

fn is_cell(place: &str) -> bool {
    let row = place.trim_start_matches(|letter: char| letter.is_ascii_alphabetic());
    row.len() < place.len() && !row.is_empty() && row.bytes().all(|digit| digit.is_ascii_digit())
}

fn ops_change(kind: &str, ops: &Value, track: bool, author: &str) -> Value {
    if kind == "word" {
        json!({ "kind": "ops", "ops": ops, "track": track, "author": author })
    } else {
        json!({ "kind": "ops", "ops": ops })
    }
}

fn parse_ops(text: &str) -> Result<Value, Failed> {
    let source = if text.trim_start().starts_with(['[', '{']) {
        text.to_owned()
    } else {
        std::fs::read_to_string(text)?
    };
    let parsed: Value = serde_json::from_str(&source)
        .map_err(|error| Failed::Says(format!("The ops are not JSON: {error}")))?;
    Ok(if parsed.is_array() {
        parsed
    } else {
        json!([parsed])
    })
}

fn guide_of(kind: &str) -> Result<Guide, Failed> {
    match kind {
        "word" => Ok(Word::guide()),
        "excel" => Ok(Excel::guide()),
        "powerpoint" => Ok(PowerPoint::guide()),
        "pdf" => Ok(Pdf::guide()),
        other => Err(Failed::Says(format!("There is no format {other}."))),
    }
}

fn guide_text(guide: &Guide, op: Option<&str>) -> Result<String, Failed> {
    let mut text = String::new();
    let mut found = false;
    for item in &guide.ops {
        if op.is_some_and(|op| op != item.name) {
            continue;
        }
        found = true;
        let _ = writeln!(text, "{}: {}", item.name, item.does);
        for field in &item.fields {
            let need = if field.required {
                "required"
            } else {
                "optional"
            };
            let _ = writeln!(
                text,
                "  {} ({}, {need}): {}",
                field.name, field.kind, field.does
            );
        }
    }
    if found {
        Ok(text)
    } else {
        Err(Failed::Says(format!(
            "The format {} has no op {}.",
            guide.format,
            op.unwrap_or_default()
        )))
    }
}

fn new_file(file: &Path, from: Option<&Path>) -> Result<String, Failed> {
    if file.exists() {
        return Err(Failed::Says(format!("The file {} exists.", file.display())));
    }
    let source = from.map(std::fs::read_to_string).transpose()?;
    let extension = from
        .and_then(Path::extension)
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bytes = match kind_of(file)? {
        "word" => word::new_file(source.as_deref()),
        "excel" => excel::new_file(source.as_deref().map(|text| (extension.as_str(), text))),
        "powerpoint" => powerpoint::new_file(source.as_deref()),
        _ => Err("This command makes Word, Excel and PowerPoint files.".to_owned()),
    }
    .map_err(Failed::Says)?;
    std::fs::write(file, bytes)?;
    Ok(format!("Made {}.\n", file.display()))
}

fn random_client() -> Result<String, Failed> {
    let mut bytes = [0_u8; 8];
    getrandom::fill(&mut bytes).map_err(|error| Failed::Io(std::io::Error::other(error)))?;
    Ok(format!("agent-{}", u64::from_be_bytes(bytes)))
}

/// One agent's place in one document.
struct Seat {
    pipe: Pipe,
    rev: u64,
    seq: u64,
    next_query: u64,
    peers: HashMap<String, Peer>,
    used_at: Instant,
}

impl Seat {
    async fn open(documents: &Documents, path: &Path, viewer: Viewer) -> Result<Self, Failed> {
        let mut pipe = documents.open(path, viewer).await?;
        let join = ToDocument::Join {
            contract: CONTRACT,
            client: random_client()?,
            epoch: 0,
            rev: 0,
        };
        send(&pipe, &join).await?;
        loop {
            match message(&mut pipe).await? {
                ToClient::Joined {
                    rev,
                    seq,
                    peers,
                    catchup: Catchup::Snapshot { .. } | Catchup::Missed { .. },
                    ..
                } => {
                    return Ok(Self {
                        pipe,
                        rev,
                        seq,
                        next_query: 1,
                        peers: peers
                            .into_iter()
                            .map(|peer| (peer.id.clone(), peer))
                            .collect(),
                        used_at: Instant::now(),
                    });
                }
                ToClient::Closed { reason } => {
                    return Err(Failed::Says(format!("The document closed: {reason:?}.")));
                }
                _ => {}
            }
        }
    }

    fn note(&mut self, message: ToClient) {
        match message {
            ToClient::Changed(changed) => self.rev = changed.rev,
            ToClient::Peer(peer) => {
                self.peers.insert(peer.id.clone(), peer);
            }
            ToClient::Left { id } => {
                self.peers.remove(&id);
            }
            _ => {}
        }
    }

    async fn query(&mut self, body: Value) -> Result<(u64, Value), Failed> {
        let id = self.next_query;
        self.next_query += 1;
        send(&self.pipe, &ToDocument::Query { id, body }).await?;
        loop {
            match message(&mut self.pipe).await? {
                ToClient::Answer {
                    id: answered,
                    rev,
                    body,
                } if answered == id => {
                    return Ok((rev, body));
                }
                other => self.note(other),
            }
        }
    }

    async fn apply(&mut self, base: u64, body: Value) -> Result<u64, Failed> {
        self.seq += 1;
        let seq = self.seq;
        send(&self.pipe, &ToDocument::Change { seq, base, body }).await?;
        loop {
            match message(&mut self.pipe).await? {
                ToClient::Changed(changed) if changed.seq == seq && changed.rev > self.rev => {
                    self.rev = changed.rev;
                    return Ok(changed.rev);
                }
                ToClient::Refused {
                    seq: refused,
                    detail,
                    reason,
                } if refused == seq => {
                    self.seq -= 1;
                    return Err(Failed::Refused(detail.unwrap_or_else(|| {
                        format!("The change was refused: {reason:?}. Read again.")
                    })));
                }
                other => self.note(other),
            }
        }
    }
}

async fn send(pipe: &Pipe, message: &ToDocument) -> Result<(), Failed> {
    pipe.send(contract::encode_message(message))
        .await
        .map_err(|closed| Failed::Says(closed.to_string()))
}

async fn message(pipe: &mut Pipe) -> Result<ToClient, Failed> {
    loop {
        let frame = tokio::time::timeout(ANSWER_LIMIT, pipe.receive())
            .await
            .map_err(|_| Failed::Says("The document did not answer.".to_owned()))?
            .ok_or_else(|| Failed::Says("The document closed.".to_owned()))?;
        match contract::decode::<ToClient>(&frame) {
            Ok(Frame::Message(message)) => return Ok(message),
            Ok(Frame::Asset { .. }) => {}
            Err(error) => return Err(Failed::Says(error.to_string())),
        }
    }
}

/// The agents of one host, each with its seats in the documents that it works on.
pub struct Agents {
    documents: Documents,
    seats: Arc<Mutex<HashMap<(String, PathBuf), Arc<tokio::sync::Mutex<Seat>>>>>,
}

impl Agents {
    pub fn new(documents: Documents) -> Self {
        Self {
            documents,
            seats: Arc::default(),
        }
    }

    fn lock(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<(String, PathBuf), Arc<tokio::sync::Mutex<Seat>>>> {
        self.seats.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lets go of seats that no command used for a while.
    fn forget_old(&self) {
        self.lock().retain(|_, seat| {
            seat.try_lock()
                .map_or(true, |seat| seat.used_at.elapsed() < SEAT_KEPT)
        });
    }

    async fn seat(
        &self,
        viewer: &Viewer,
        path: &Path,
    ) -> Result<Arc<tokio::sync::Mutex<Seat>>, Failed> {
        self.forget_old();
        let key = (viewer.id.clone(), std::path::absolute(path)?);
        let known = self.lock().get(&key).cloned();
        if let Some(seat) = known {
            return Ok(seat);
        }
        let seat = Arc::new(tokio::sync::Mutex::new(
            Seat::open(&self.documents, path, viewer.clone()).await?,
        ));
        self.lock().insert(key, Arc::clone(&seat));
        Ok(seat)
    }

    /// Runs one command and gives the text that the agent reads.
    pub async fn run(&self, viewer: &Viewer, command: Command) -> Result<String, Failed> {
        match command {
            Command::New(arguments) => new_file(&arguments.file, arguments.from.as_deref()),
            Command::Guide(arguments) => {
                guide_text(&guide_of(&arguments.format)?, arguments.op.as_deref())
            }
            Command::Read(arguments) => {
                let kind = kind_of(&arguments.file)?;
                let seat = self.seat(viewer, &arguments.file).await?;
                let mut seat = seat.lock().await;
                let body = read_query(kind, arguments.place.as_deref());
                let (rev, answer) = seat.query(body).await?;
                seat.used_at = Instant::now();
                drop(seat);
                Ok(pretty(&json!({ "rev": rev, "content": answer })))
            }
            Command::Check(arguments) => {
                kind_of(&arguments.file)?;
                let seat = self.seat(viewer, &arguments.file).await?;
                let mut seat = seat.lock().await;
                let (rev, answer) = seat.query(json!({ "kind": "check" })).await?;
                seat.used_at = Instant::now();
                drop(seat);
                Ok(pretty(&json!({ "rev": rev, "problems": answer })))
            }
            Command::Apply(arguments) => {
                let kind = kind_of(&arguments.file)?;
                let ops = parse_ops(&arguments.ops)?;
                let seat = self.seat(viewer, &arguments.file).await?;
                let mut seat = seat.lock().await;
                let body = ops_change(kind, &ops, arguments.track, &viewer.name);
                let rev = seat.apply(arguments.base, body).await?;
                seat.used_at = Instant::now();
                drop(seat);
                Ok(pretty(&json!({ "rev": rev })))
            }
            Command::Selection(arguments) => {
                kind_of(&arguments.file)?;
                let seat = self.seat(viewer, &arguments.file).await?;
                let mut seat = seat.lock().await;
                seat.query(json!({ "kind": "check" })).await?;
                let people: Vec<Value> = seat
                    .peers
                    .values()
                    .map(|peer| json!({ "name": peer.name, "place": peer.place }))
                    .collect();
                seat.used_at = Instant::now();
                drop(seat);
                Ok(pretty(&json!({ "people": people })))
            }
        }
    }
}

fn pretty(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests;
