//! The agent's commands. An agent is one more client of the contract: a command joins a live
//! document as a viewer in the agent's name, and its edits show at once, as a person's do. A
//! host keeps the agent's seat for a short time after the last command, so the agent stays among
//! the people who are here.

use std::{
    collections::HashMap,
    fmt::Write as _,
    io::Read as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
    time::Duration,
};

use clap::{Args, Subcommand};
use document::{CONTRACT, Catchup, Format, Frame, Guide, ToClient, ToDocument, contract};
use documents::{Documents, Formats, OpenError, Pipe, Viewer, format};
use docx::Docx;
use pdf::Pdf;
use plain::Plain;
use pptx::Pptx;
use serde_json::{Value, json};
use tokio::time::Instant;
use xlsx::Xlsx;

const ANSWER_LIMIT: Duration = Duration::from_mins(1);
/// How long an agent stays among the people after its last command.
const SEAT_KEPT: Duration = Duration::from_secs(30);
/// How often a host looks for seats that no command used for a while.
const FORGET_EVERY: Duration = Duration::from_secs(5);
/// The largest file of ops or text to start from that an agent can give.
const MOST_TEXT: u64 = 32 * 1024 * 1024;

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
    /// Makes a .docx, .xlsx or .pptx file.
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
    /// A block id, a cell range such as Sheet1!A1:C9, a slide id, a page number, or lines of a
    /// text file such as 10-20.
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
    /// Makes your text changes in a .docx file suggestions; other changes apply directly.
    #[arg(long)]
    pub track: bool,
}

#[derive(Debug, Clone, Args)]
pub struct FileArguments {
    pub file: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct GuideArguments {
    /// docx, xlsx, pptx, pdf or text.
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

/// The extensions of plain text files, which open as text.
const TEXT_EXTENSIONS: [&str; 6] = ["txt", "json", "yaml", "yml", "toml", "xml"];

/// Each format, by file extension.
pub fn formats() -> Formats {
    TEXT_EXTENSIONS.into_iter().fold(
        Formats::default()
            .with("docx", format::<Docx>())
            .with("xlsx", format::<Xlsx>())
            .with("pptx", format::<Pptx>())
            .with("pdf", format::<Pdf>()),
        |formats, extension| formats.with(extension, format::<Plain>()),
    )
}

fn kind_of(path: &Path) -> Result<&'static str, Failed> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if TEXT_EXTENSIONS.contains(&extension.as_str()) {
        return Ok("text");
    }
    ["docx", "xlsx", "pptx", "pdf"]
        .into_iter()
        .find(|kind| *kind == extension)
        .ok_or_else(|| {
            Failed::Says(
                "This command takes .docx, .xlsx, .pptx and .pdf files, and text files such as \
                 .txt, .json, .yaml, .yml, .toml and .xml."
                    .to_owned(),
            )
        })
}

fn read_query(kind: &str, place: Option<&str>) -> Result<Value, Failed> {
    Ok(match (kind, place) {
        ("text", place) => {
            let (from, to) = place.map_or(Ok((None, None)), lines_of)?;
            json!({ "kind": "read", "from": from, "to": to })
        }
        ("docx", place) => json!({ "kind": "read", "block": place }),
        ("xlsx", Some(place)) => match place.split_once('!') {
            Some((sheet, range)) => json!({ "kind": "read", "sheet": sheet, "range": range }),
            None if place.contains(':') || is_cell(place) => {
                json!({ "kind": "read", "range": place })
            }
            None => json!({ "kind": "read", "sheet": place }),
        },
        ("xlsx", None) => json!({ "kind": "read" }),
        ("pptx", place) => json!({ "kind": "read", "slide": place }),
        (_, place) => json!({
            "kind": "read",
            "page": place.and_then(|page| page.parse::<u32>().ok()),
        }),
    })
}

/// The lines that `--where` names in a text file: one line, `10-20`, or `10-` to the end.
fn lines_of(place: &str) -> Result<(Option<usize>, Option<usize>), Failed> {
    let number = |text: &str| text.trim().parse::<usize>().ok().filter(|line| *line > 0);
    let lines = match place.split_once('-') {
        Some((from, "")) => number(from).map(|from| (Some(from), None)),
        Some((from, to)) => number(from)
            .zip(number(to))
            .filter(|(from, to)| from <= to)
            .map(|(from, to)| (Some(from), Some(to))),
        None => number(place).map(|line| (Some(line), Some(line))),
    };
    lines.ok_or_else(|| {
        Failed::Says(format!(
            "{place} names no lines. Give a line or lines, such as 12, 10-20 or 10-."
        ))
    })
}

fn is_cell(place: &str) -> bool {
    let row = place.trim_start_matches(|letter: char| letter.is_ascii_alphabetic());
    row.len() < place.len() && !row.is_empty() && row.bytes().all(|digit| digit.is_ascii_digit())
}

fn ops_change(kind: &str, ops: &Value, track: bool, author: &str) -> Value {
    if kind == "docx" {
        json!({ "kind": "ops", "ops": ops, "track": track, "author": author })
    } else {
        json!({ "kind": "ops", "ops": ops })
    }
}

fn parse_ops(text: &str) -> Result<Value, Failed> {
    let source = if text.trim_start().starts_with(['[', '{']) {
        text.to_owned()
    } else {
        read_text(Path::new(text))?
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
    match kind.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "docx" => Ok(Docx::guide()),
        "xlsx" => Ok(Xlsx::guide()),
        "pptx" => Ok(Pptx::guide()),
        "pdf" => Ok(Pdf::guide()),
        kind if kind == "text" || TEXT_EXTENSIONS.contains(&kind) => Ok(Plain::guide()),
        other => Err(Failed::Says(format!(
            "There is no format {other}. Use docx, xlsx, pptx, pdf or text."
        ))),
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
        let shape = std::iter::once(format!("\"op\": \"{}\"", item.name))
            .chain(
                item.fields
                    .iter()
                    .filter(|field| field.required)
                    .map(|field| format!("\"{}\": …", field.name)),
            )
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(text, "  {{{shape}}}");
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
    let source = from.map(read_text).transpose()?;
    let extension = from
        .and_then(Path::extension)
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bytes = match kind_of(file)? {
        "docx" => docx::new_file(source.as_deref()),
        "xlsx" => xlsx::new_file(source.as_deref().map(|text| (extension.as_str(), text))),
        "pptx" => pptx::new_file(source.as_deref()),
        _ => Err("This command makes .docx, .xlsx and .pptx files.".to_owned()),
    }
    .map_err(Failed::Says)?;
    std::fs::write(file, bytes)?;
    Ok(format!("Made {}.\n", file.display()))
}

/// The text of a file that an agent names. The host reads it, so it reads only a file of a
/// bounded size.
fn read_text(path: &Path) -> Result<String, Failed> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(Failed::Says(format!("{} is not a file.", path.display())));
    }
    let mut text = String::new();
    std::fs::File::open(path)?
        .take(MOST_TEXT + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MOST_TEXT {
        return Err(Failed::Says(format!(
            "{} is larger than {} MB.",
            path.display(),
            MOST_TEXT / 1024 / 1024
        )));
    }
    Ok(text)
}

fn random_client() -> Result<String, Failed> {
    let mut bytes = [0_u8; 8];
    getrandom::fill(&mut bytes).map_err(|error| Failed::Io(std::io::Error::other(error)))?;
    Ok(format!("agent-{}", u64::from_be_bytes(bytes)))
}

/// One agent's place in one document.
struct Seat {
    pipe: Pipe,
    client: String,
    rev: u64,
    seq: u64,
    next_query: u64,
    used_at: Instant,
}

impl Seat {
    async fn open(documents: &Documents, path: &Path, viewer: Viewer) -> Result<Self, Failed> {
        let mut pipe = documents.open(path, viewer).await?;
        let client = random_client()?;
        let join = ToDocument::Join {
            contract: CONTRACT,
            client: client.clone(),
            epoch: 0,
            rev: 0,
        };
        send(&pipe, &join).await?;
        loop {
            match message(&mut pipe).await? {
                ToClient::Joined {
                    rev,
                    seq,
                    catchup: Catchup::Snapshot { .. } | Catchup::Missed { .. },
                    ..
                } => {
                    return Ok(Self {
                        pipe,
                        client,
                        rev,
                        seq,
                        next_query: 1,
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
        if let ToClient::Changed(changed) = message {
            self.rev = changed.rev;
        }
    }

    async fn query(&mut self, body: Value) -> Result<(u64, Value), Failed> {
        self.ask(|id| ToDocument::Query { id, body }).await
    }

    async fn ask(&mut self, asked: impl FnOnce(u64) -> ToDocument) -> Result<(u64, Value), Failed> {
        let id = self.next_query;
        self.next_query += 1;
        send(&self.pipe, &asked(id)).await?;
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
                ToClient::Changed(changed)
                    if changed.client == self.client && changed.seq == seq =>
                {
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

type Seats = HashMap<(String, PathBuf), Arc<tokio::sync::Mutex<Seat>>>;

fn lock(seats: &Mutex<Seats>) -> MutexGuard<'_, Seats> {
    seats.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Lets go of the seats that no command used for a while, until no seat is left.
async fn forget_idle(seats: Weak<Mutex<Seats>>) {
    let mut every = tokio::time::interval(FORGET_EVERY);
    loop {
        every.tick().await;
        let Some(seats) = seats.upgrade() else {
            return;
        };
        let mut seats = lock(&seats);
        seats.retain(|_, seat| {
            seat.try_lock()
                .map_or(true, |seat| seat.used_at.elapsed() < SEAT_KEPT)
        });
        if seats.is_empty() {
            return;
        }
    }
}

/// The agents of one host, each with its seats in the documents that it works on.
pub struct Agents {
    documents: Documents,
    seats: Arc<Mutex<Seats>>,
}

impl Agents {
    pub fn new(documents: Documents) -> Self {
        Self {
            documents,
            seats: Arc::default(),
        }
    }

    async fn seat(
        &self,
        viewer: &Viewer,
        path: &Path,
    ) -> Result<Arc<tokio::sync::Mutex<Seat>>, Failed> {
        let key = (viewer.id.clone(), documents::real_path(path)?);
        let known = lock(&self.seats).get(&key).cloned();
        if let Some(seat) = known {
            return Ok(seat);
        }
        let seat = Arc::new(tokio::sync::Mutex::new(
            Seat::open(&self.documents, path, viewer.clone()).await?,
        ));
        let mut seats = lock(&self.seats);
        if seats.is_empty() {
            tokio::spawn(forget_idle(Arc::downgrade(&self.seats)));
        }
        seats.insert(key, Arc::clone(&seat));
        drop(seats);
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
                let body = read_query(kind_of(&arguments.file)?, arguments.place.as_deref())?;
                let seat = self.seat(viewer, &arguments.file).await?;
                let mut seat = seat.lock().await;
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
                let (_, people) = seat.ask(|id| ToDocument::People { id }).await?;
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
