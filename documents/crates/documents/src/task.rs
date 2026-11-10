use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use document::{ClosedReason, Effect, Hash, Input, PipeId, Rev, Viewer};
use sha2::{Digest as _, Sha256};
use tokio::{
    sync::{mpsc, oneshot},
    time::{Instant, MissedTickBehavior},
};

use crate::{
    OpenError,
    journal::{Found, Journal},
    live::{Live, Opener},
};

const SAVE_AFTER_LAST: Duration = Duration::from_secs(2);
const SAVE_AFTER_FIRST: Duration = Duration::from_secs(15);
const CLOSE_AFTER: Duration = Duration::from_mins(1);
const RETRY_SAVE: Duration = Duration::from_secs(1);
const WATCH_EVERY: Duration = Duration::from_secs(1);
const MOST_WAITING: usize = 8 * 1024 * 1024;
const MOST_JOURNAL: u64 = 64 * 1024 * 1024;
const COMMANDS: usize = 256;
const BATCH: usize = 64;
const OUTSIDE: &str = "Another program";

pub(crate) enum Command {
    Connect {
        pipe: PipeId,
        viewer: Viewer,
        outgoing: mpsc::UnboundedSender<Arc<[u8]>>,
        waiting: Arc<AtomicUsize>,
    },
    Input(Input),
    Close {
        done: oneshot::Sender<()>,
    },
}

struct Outgoing {
    frames: mpsc::UnboundedSender<Arc<[u8]>>,
    waiting: Arc<AtomicUsize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    size: u64,
    changed: Option<SystemTime>,
}

fn stamp_of(metadata: &std::fs::Metadata) -> Stamp {
    Stamp {
        size: metadata.len(),
        changed: metadata.modified().ok(),
    }
}

fn hash_of(bytes: &[u8]) -> Hash {
    Sha256::digest(bytes).into()
}

fn hex(hash: &Hash) -> String {
    hash.iter()
        .fold(String::new(), |text, byte| text + &format!("{byte:02x}"))
}

fn new_epoch() -> io::Result<u64> {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes[2..]).map_err(io::Error::other)?;
    Ok(u64::from_be_bytes(bytes))
}

/// Writes a file in one step: nobody sees half of it, and its rights stay.
async fn write_whole(path: &Path, bytes: &[u8]) -> io::Result<std::fs::Metadata> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = path.with_file_name(format!(".{name}.saving"));
    let result = async {
        let current = tokio::fs::metadata(path).await?;
        tokio::fs::write(&temporary, bytes).await?;
        // Windows flushes a file only through a handle that may write it.
        tokio::fs::OpenOptions::new()
            .write(true)
            .open(&temporary)
            .await?
            .sync_data()
            .await?;
        tokio::fs::set_permissions(&temporary, current.permissions()).await?;
        tokio::fs::rename(&temporary, path).await?;
        tokio::fs::metadata(path).await
    }
    .await;
    if result.is_err() {
        drop(tokio::fs::remove_file(&temporary).await);
    }
    result
}

pub(crate) async fn start(
    path: PathBuf,
    folder: PathBuf,
    format: Arc<dyn Opener>,
) -> Result<mpsc::Sender<Command>, OpenError> {
    let file = tokio::fs::read(&path).await?;
    let stamp = stamp_of(&tokio::fs::metadata(&path).await?);
    let (document, journal, journaled) = open_or_restore(&path, &folder, &file, format.as_ref())?;
    let (disk, stamp) = journaled.map_or((file, Some(stamp)), |original| (original, None));
    let unsaved = (document.rev() != document.saved_rev()).then(Instant::now);
    let (commands, incoming) = mpsc::channel(COMMANDS);
    let task = Task {
        document,
        journal,
        path,
        folder,
        disk,
        stamp,
        pipes: HashMap::new(),
        first_unsaved: unsaved,
        last_unsaved: unsaved,
        retry_at: None,
        close_at: Some(Instant::now() + CLOSE_AFTER),
        sealing: false,
    };
    tokio::spawn(task.run(incoming));
    Ok(commands)
}

/// Opens a file, or builds its document again from the journal of an earlier run. When another
/// program changed the file since the journal's last save, it also gives the file that the
/// journal holds, so the task takes the file on disk as a write of that program.
fn open_or_restore(
    path: &Path,
    folder: &Path,
    disk: &[u8],
    format: &dyn Opener,
) -> Result<(Box<dyn Live>, Journal, Option<Vec<u8>>), OpenError> {
    if let Some((mut journal, found)) = Journal::find(folder, path) {
        let Found {
            original,
            restored,
            records,
            assets,
            saved_hash,
        } = found;
        let known = saved_hash.unwrap_or_else(|| hash_of(&original));
        let journaled = (hash_of(disk) != known).then(|| original.clone());
        match format.restore(original, restored, &records, assets) {
            Ok(document) => return Ok((document, journal, journaled)),
            Err(problem) => {
                tracing::warn!(%problem, "the journal is not usable, so the file opens as it is");
                journal.remove();
            }
        }
    }
    let epoch = new_epoch()?;
    let document = format.open(disk.to_vec(), epoch)?;
    let journal = Journal::start(folder, path, disk, epoch, 0, HashMap::new())?;
    Ok((document, journal, None))
}

struct Task {
    document: Box<dyn Live>,
    journal: Journal,
    path: PathBuf,
    folder: PathBuf,
    disk: Vec<u8>,
    /// The size and time of the file when the task last read or wrote it.
    stamp: Option<Stamp>,
    pipes: HashMap<PipeId, Outgoing>,
    first_unsaved: Option<Instant>,
    last_unsaved: Option<Instant>,
    retry_at: Option<Instant>,
    close_at: Option<Instant>,
    sealing: bool,
}

impl Task {
    async fn run(mut self, mut commands: mpsc::Receiver<Command>) {
        if self.look_at_file().await {
            return;
        }
        let mut watch = tokio::time::interval_at(Instant::now() + WATCH_EVERY, WATCH_EVERY);
        watch.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            let deadline = self.next_deadline();
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break };
                    let mut batch = vec![command];
                    while batch.len() < BATCH {
                        match commands.try_recv() {
                            Ok(command) => batch.push(command),
                            Err(_) => break,
                        }
                    }
                    if self.take(batch).await {
                        return;
                    }
                }
                () = sleep_until(deadline) => {
                    if self.deadline_reached().await {
                        return;
                    }
                }
                _ = watch.tick() => {
                    if self.look_at_file().await {
                        return;
                    }
                }
            }
        }
        self.finish(ClosedReason::Failed).await;
    }

    async fn take(&mut self, batch: Vec<Command>) -> bool {
        let mut effects = Vec::new();
        for command in batch {
            match command {
                Command::Connect {
                    pipe,
                    viewer,
                    outgoing,
                    waiting,
                } => {
                    self.close_at = None;
                    self.pipes.insert(
                        pipe,
                        Outgoing {
                            frames: outgoing,
                            waiting,
                        },
                    );
                    effects.extend(self.document.step(Input::Connected { pipe, viewer }));
                }
                Command::Input(input) => {
                    if let Input::Left { pipe } = &input {
                        self.pipes.remove(pipe);
                    }
                    effects.extend(self.document.step(input));
                }
                Command::Close { done } => {
                    self.run_effects(std::mem::take(&mut effects)).await;
                    self.finish(ClosedReason::Failed).await;
                    let _sent = done.send(());
                    return true;
                }
            }
        }
        self.run_effects(effects).await;
        if self.document.is_empty() && self.close_at.is_none() {
            self.close_at = Some(Instant::now() + CLOSE_AFTER);
        }
        false
    }

    fn next_deadline(&self) -> Option<Instant> {
        let saving = self
            .first_unsaved
            .zip(self.last_unsaved)
            .map(|(first, last)| (first + SAVE_AFTER_FIRST).min(last + SAVE_AFTER_LAST));
        [saving, self.retry_at, self.close_at]
            .into_iter()
            .flatten()
            .min()
    }

    async fn deadline_reached(&mut self) -> bool {
        let now = Instant::now();
        if self.close_at.is_some_and(|at| at <= now) {
            self.finish(ClosedReason::Failed).await;
            return true;
        }
        self.retry_at = None;
        self.save().await;
        false
    }

    /// Runs what a document asked for. Each change is safe in the journal before any view hears
    /// of it.
    async fn run_effects(&mut self, effects: Vec<Effect>) {
        let mut appended = false;
        let mut sends = Vec::new();
        for effect in effects {
            match effect {
                Effect::Append(record) => {
                    if let Err(problem) = self.journal.append(&record) {
                        tracing::warn!(%problem, "the journal cannot grow");
                    }
                    appended = true;
                }
                Effect::KeepAsset(hash) => {
                    if let Some(bytes) = self.document.asset(&hash)
                        && let Err(problem) = self.journal.keep_asset(&hex(&hash), &bytes)
                    {
                        tracing::warn!(%problem, "an asset stays only in memory");
                    }
                }
                Effect::Unsaved => {
                    let now = Instant::now();
                    self.first_unsaved.get_or_insert(now);
                    self.last_unsaved = Some(now);
                }
                Effect::Seal => self.sealing = true,
                other => sends.push(other),
            }
        }
        if appended && let Err(problem) = self.journal.sync() {
            tracing::warn!(%problem, "the journal did not reach the disk");
        }
        for effect in sends {
            match effect {
                Effect::Send { to, frame, catchup } => self.send(&to, &frame, catchup),
                Effect::Drop { pipe } => {
                    self.pipes.remove(&pipe);
                    let more = self.document.step(Input::Left { pipe });
                    self.send_all(more);
                }
                Effect::Write { rev, file } => self.write(rev, file).await,
                _ => {}
            }
        }
        if self.sealing || self.journal.bytes() > MOST_JOURNAL {
            self.sealing = false;
            self.save().await;
            if self.document.rev() == self.document.saved_rev() {
                self.start_new_journal();
            }
        }
    }

    fn send_all(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            if let Effect::Send { to, frame, catchup } = effect {
                self.send(&to, &frame, catchup);
            }
        }
    }

    fn send(&mut self, to: &[PipeId], frame: &Arc<[u8]>, catchup: bool) {
        let mut behind = Vec::new();
        for pipe in to {
            let Some(outgoing) = self.pipes.get(pipe) else {
                continue;
            };
            let waiting = outgoing.waiting.load(Ordering::Relaxed);
            if !catchup && waiting > MOST_WAITING {
                behind.push(*pipe);
                continue;
            }
            outgoing.waiting.fetch_add(frame.len(), Ordering::Relaxed);
            if outgoing.frames.send(Arc::clone(frame)).is_err() {
                behind.push(*pipe);
            }
        }
        for pipe in behind {
            self.pipes.remove(&pipe);
            let more = self.document.step(Input::Left { pipe });
            self.send_all(more);
        }
    }

    async fn save(&mut self) {
        let file = self.disk.clone();
        let effects = self.document.step(Input::Save { file });
        self.first_unsaved = None;
        self.last_unsaved = None;
        for effect in effects {
            match effect {
                Effect::Write { rev, file } => self.write(rev, file).await,
                Effect::Send { to, frame, catchup } => {
                    self.retry_at = Some(Instant::now() + RETRY_SAVE);
                    self.send(&to, &frame, catchup);
                }
                _ => {}
            }
        }
    }

    async fn write(&mut self, rev: Rev, file: Vec<u8>) {
        match write_whole(&self.path, &file).await {
            Ok(metadata) => {
                self.stamp = Some(stamp_of(&metadata));
                if let Err(problem) = self.journal.saved(rev, &hash_of(&file)) {
                    tracing::warn!(%problem, "the journal missed a save");
                }
                self.disk = file;
                let effects = self.document.step(Input::Saved { rev });
                self.send_all(effects);
            }
            Err(problem) => {
                tracing::debug!(%problem, "the save waits");
                self.retry_at = Some(Instant::now() + RETRY_SAVE);
                let effects = self.document.step(Input::SaveWaits);
                self.send_all(effects);
            }
        }
    }

    fn start_new_journal(&mut self) {
        let Ok(epoch) = new_epoch() else {
            return;
        };
        let started = Journal::start(
            &self.folder,
            &self.path,
            &self.disk,
            epoch,
            self.document.rev(),
            self.document.seqs(),
        );
        match started {
            Ok(journal) => {
                self.journal = journal;
                let file = self.disk.clone();
                let effects = self.document.step(Input::Rebased { file, epoch });
                self.send_all(effects);
            }
            Err(problem) => tracing::warn!(%problem, "the journal goes on"),
        }
    }

    /// Looks at the file, as one quick check of its size and time, and takes a write of another
    /// program. It returns true when the file is gone.
    async fn look_at_file(&mut self) -> bool {
        let metadata = match tokio::fs::metadata(&self.path).await {
            Ok(metadata) => metadata,
            Err(problem) if problem.kind() == io::ErrorKind::NotFound => {
                self.remove_gone();
                return true;
            }
            Err(_) => return false,
        };
        let stamp = Some(stamp_of(&metadata));
        if stamp == self.stamp {
            return false;
        }
        let Ok(file) = tokio::fs::read(&self.path).await else {
            return false;
        };
        self.stamp = stamp;
        if hash_of(&file) == hash_of(&self.disk) {
            return false;
        }
        let effects = self.document.step(Input::FileChanged {
            file: file.clone(),
            writer: OUTSIDE.to_owned(),
        });
        if effects.contains(&Effect::FileTaken) {
            self.disk = file;
        }
        self.run_effects(effects).await;
        false
    }

    fn remove_gone(&mut self) {
        let effects = self.document.step(Input::Closing {
            reason: ClosedReason::Removed,
        });
        self.send_all(effects);
    }

    async fn finish(&mut self, reason: ClosedReason) {
        self.save().await;
        let saved = self.document.rev() == self.document.saved_rev();
        let effects = self.document.step(Input::Closing { reason });
        self.send_all(effects);
        if saved {
            self.journal.remove();
        }
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
