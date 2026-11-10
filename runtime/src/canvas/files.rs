use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock,
        atomic::{AtomicU64, Ordering},
    },
    time::SystemTime,
};

use base64::Engine as _;
use bytes::Bytes;

use super::{
    CanvasAnswer, CanvasItem, CanvasKind, DocumentView, MOST_FILE_BYTES, MOST_ITEMS, MOST_KEY,
    PART_BYTES, kind_of, page,
};
use crate::text::Text;

const MOST_PAGE_SOURCE: u64 = 8 * 1024 * 1024;
const NAME_CHARACTERS: usize = 128;
const SNAPSHOT_KEPT: std::time::Duration = std::time::Duration::from_mins(1);
/// A change after this long without one opens the canvas at the file again.
pub(super) const QUIET: std::time::Duration = std::time::Duration::from_mins(1);

/// The formats of live documents, which say whether a text file opens live.
static FORMATS: LazyLock<documents::Formats> = LazyLock::new(documents_cli::formats);

/// Counts the changes of a canvas, so devices know the newest item and what changed since they
/// looked.
#[derive(Clone, Default)]
pub(super) struct Stamp(Arc<AtomicU64>);

impl Stamp {
    pub(super) fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(super) fn now(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Seen {
    bytes: u64,
    modified: Option<SystemTime>,
}

struct Item {
    path: PathBuf,
    listed: CanvasItem,
    seen: Option<Seen>,
    /// When the file was shown or last changed.
    touched: Option<tokio::time::Instant>,
}

impl Item {
    /// Notes a change, and says whether it comes after a quiet while.
    fn touch(&mut self) -> bool {
        let quiet = self.touched.is_none_or(|at| at.elapsed() >= QUIET);
        self.touched = Some(tokio::time::Instant::now());
        quiet
    }
}

/// What a check of the files found.
pub(super) struct Checked {
    pub(super) any: bool,
    /// The files whose change comes after a quiet while, newest first.
    pub(super) after_quiet: Vec<String>,
}

/// A file that the canvas can show.
pub(super) struct Examined {
    path: PathBuf,
    kind: CanvasKind,
    view: Option<DocumentView>,
    seen: Seen,
    /// The first address that a page loads from the network.
    pub loads: Option<String>,
}

impl Examined {
    pub(super) fn name(&self) -> String {
        name_of(&self.path)
    }

    /// The address of the file, for the shared browser.
    pub(super) fn address(&self) -> Option<String> {
        reqwest::Url::from_file_path(&self.path)
            .ok()
            .map(String::from)
    }
}

struct Snapshot {
    key: String,
    revision: u64,
    bytes: Bytes,
    used: tokio::time::Instant,
}

pub(super) struct Shown {
    pub key: String,
    pub name: String,
    pub loads: Option<String>,
}

pub(super) struct Showing {
    pub shown: Vec<Shown>,
    pub dropped: Vec<String>,
}

#[derive(Default)]
pub(super) struct Files {
    items: Vec<Item>,
    snapshot: Option<Snapshot>,
}

/// Checks each file before the canvas shows any of them.
pub(super) async fn examine_all(paths: Vec<PathBuf>) -> Result<Vec<Examined>, Text> {
    let mut checked = Vec::with_capacity(paths.len());
    for path in paths {
        checked.push(examine(path).await?);
    }
    Ok(checked)
}

impl Files {
    pub(super) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub(super) fn contains(&self, key: &str) -> bool {
        self.items.iter().any(|item| item.listed.key == key)
    }

    pub(super) fn items(&self) -> Vec<CanvasItem> {
        self.items.iter().map(|item| item.listed.clone()).collect()
    }

    /// Puts each file on the canvas, or brings it to the front. The newest comes first.
    pub(super) fn show(&mut self, files: Vec<Examined>, stamp: &Stamp) -> Showing {
        let mut shown = Vec::new();
        for Examined {
            path,
            kind,
            view,
            seen,
            loads,
        } in files
        {
            let key = key_of(&path);
            let name = name_of(&path);
            let changed = stamp.next();
            let item = match self.items.iter().position(|item| item.listed.key == key) {
                Some(at) => {
                    let mut item = self.items.remove(at);
                    if item.seen.as_ref() != Some(&seen) {
                        item.listed.revision = changed;
                    }
                    item.listed.view = view;
                    item.listed.bytes = seen.bytes;
                    item.listed.missing = false;
                    item.listed.changed = changed;
                    item.seen = Some(seen);
                    item.touched = Some(tokio::time::Instant::now());
                    item
                }
                None => Item {
                    listed: CanvasItem {
                        key,
                        name: name.clone(),
                        kind,
                        view,
                        revision: changed,
                        bytes: seen.bytes,
                        changed,
                        missing: false,
                    },
                    path,
                    seen: Some(seen),
                    touched: Some(tokio::time::Instant::now()),
                },
            };
            shown.push(Shown {
                key: item.listed.key.clone(),
                name,
                loads,
            });
            self.items.insert(0, item);
        }
        let dropped = self
            .items
            .drain(MOST_ITEMS.min(self.items.len())..)
            .map(|item| item.listed.name)
            .collect();
        self.forget_unlisted_snapshot();
        Showing { shown, dropped }
    }

    /// Brings back the files of a terminal that returns. A file that is gone shows as missing.
    pub(super) fn restore(&mut self, paths: Vec<PathBuf>, stamp: &Stamp) {
        for path in paths.into_iter().take(MOST_ITEMS) {
            let Some(kind) = kind_of(&path) else { continue };
            let key = key_of(&path);
            if self.items.iter().any(|item| item.listed.key == key) {
                continue;
            }
            let changed = stamp.next();
            self.items.push(Item {
                listed: CanvasItem {
                    key,
                    name: name_of(&path),
                    kind,
                    view: super::view_of(&path),
                    revision: changed,
                    bytes: 0,
                    changed,
                    missing: false,
                },
                path,
                seen: None,
                touched: None,
            });
        }
    }

    /// Notes that the agent changed a file of the canvas, and says whether the change comes
    /// after a quiet while.
    pub(super) fn touch(&mut self, key: &str) -> bool {
        self.items
            .iter_mut()
            .find(|item| item.listed.key == key)
            .is_some_and(Item::touch)
    }

    /// Checks each file for a change. A changed file gets a new revision.
    pub(super) async fn check(&mut self, stamp: &Stamp) -> Checked {
        let mut any = false;
        let mut after_quiet = Vec::new();
        for item in &mut self.items {
            let seen = tokio::fs::metadata(&item.path)
                .await
                .ok()
                .filter(std::fs::Metadata::is_file)
                .map(|metadata| Seen {
                    bytes: metadata.len(),
                    modified: metadata.modified().ok(),
                });
            match seen {
                None if !item.listed.missing => {
                    item.listed.missing = true;
                    item.listed.changed = stamp.next();
                    any = true;
                }
                Some(seen) if item.seen.as_ref() != Some(&seen) => {
                    if item.seen.is_some() || item.listed.missing {
                        let changed = stamp.next();
                        item.listed.revision = changed;
                        item.listed.changed = changed;
                        if item.touch() {
                            after_quiet.push(item.listed.key.clone());
                        }
                    }
                    item.listed.view = view_of(&item.path, seen.bytes).await;
                    item.listed.bytes = seen.bytes;
                    item.listed.missing = false;
                    item.seen = Some(seen);
                    any = true;
                }
                _ => {}
            }
        }
        if self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.used.elapsed() > SNAPSHOT_KEPT)
        {
            self.snapshot = None;
        }
        Checked { any, after_quiet }
    }

    /// Removes the items with these keys, or all items when there are no keys.
    pub(super) fn remove(&mut self, keys: &[String]) -> usize {
        let before = self.items.len();
        self.items
            .retain(|item| !keys.is_empty() && !keys.contains(&item.listed.key));
        self.forget_unlisted_snapshot();
        before - self.items.len()
    }

    fn forget_unlisted_snapshot(&mut self) {
        self.snapshot = self
            .snapshot
            .take()
            .filter(|snapshot| self.contains(&snapshot.key));
    }

    /// One part of an item's content. Every part of one revision comes from one copy of the file.
    pub(super) async fn read(
        &mut self,
        key: &str,
        revision: u64,
        offset: u64,
    ) -> Result<CanvasAnswer, Text> {
        let item = self
            .items
            .iter()
            .find(|item| item.listed.key == key)
            .ok_or_else(|| text!("This item is no longer on the canvas."))?;
        if item.listed.missing {
            return Err(text!(
                "{name} is missing. The agent can show it again.",
                name = &item.listed.name
            ));
        }
        if item.listed.revision != revision {
            return Ok(CanvasAnswer::Stale {
                key: key.to_owned(),
                revision: item.listed.revision,
            });
        }
        let (name, kind, path) = (
            item.listed.name.clone(),
            item.listed.kind,
            item.path.clone(),
        );
        let fresh = self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.key == key && snapshot.revision == revision);
        if !fresh {
            let bytes = load(path, name.clone()).await?;
            self.snapshot = Some(Snapshot {
                key: key.to_owned(),
                revision,
                bytes,
                used: tokio::time::Instant::now(),
            });
        }
        let snapshot = self
            .snapshot
            .as_mut()
            .ok_or_else(|| text!("The canvas could not read the file."))?;
        snapshot.used = tokio::time::Instant::now();
        let total = snapshot.bytes.len();
        let start =
            usize::try_from(offset).map_err(|_| Text::plain("This part is past the end."))?;
        if start > total || (start == total && total > 0) {
            return Err(Text::plain("This part is past the end."));
        }
        let end = (start + PART_BYTES).min(total);
        Ok(CanvasAnswer::Part {
            key: key.to_owned(),
            name,
            kind,
            revision,
            offset,
            total: total as u64,
            data: base64::engine::general_purpose::STANDARD.encode(&snapshot.bytes[start..end]),
        })
    }
}

async fn examine(path: PathBuf) -> Result<Examined, Text> {
    let shown = path.display().to_string();
    let kind = kind_of(&path).ok_or_else(|| {
        Text::plain(format!(
            "{shown}: the canvas shows pages (HTML, Markdown, text), pictures, documents, sheets, presentations and PDFs."
        ))
    })?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| Text::plain(format!("There is no file {shown}.")))?;
    if !metadata.is_file() {
        return Err(Text::plain(format!("{shown} is not a file.")));
    }
    let most = if page_source(&path) {
        MOST_PAGE_SOURCE
    } else {
        MOST_FILE_BYTES
    };
    if metadata.len() > most {
        return Err(Text::plain(format!(
            "{shown} has {} MB. The canvas takes files up to {} MB.",
            metadata.len().div_ceil(1024 * 1024),
            most / 1024 / 1024
        )));
    }
    let path = kodosi_os::fs::canonicalize(&path).unwrap_or(path);
    if key_of(&path).len() > MOST_KEY {
        return Err(Text::plain(format!(
            "The path of {shown} is too long for the canvas."
        )));
    }
    let loads = if is_html(&path) {
        tokio::fs::read(&path)
            .await
            .ok()
            .and_then(|source| page::network_reference(&source))
    } else {
        None
    };
    Ok(Examined {
        view: view_of(&path, metadata.len()).await,
        path,
        kind,
        seen: Seen {
            bytes: metadata.len(),
            modified: metadata.modified().ok(),
        },
        loads,
    })
}

/// How a file of `bytes` opens on the canvas: live, or else as its kind. A text file opens live
/// only when its format takes it, and one that is too long or not UTF-8 stays a page.
async fn view_of(path: &Path, bytes: u64) -> Option<DocumentView> {
    let view = super::view_of(path)?;
    if view != DocumentView::Text {
        return Some(view);
    }
    if bytes > MOST_PAGE_SOURCE {
        return None;
    }
    let file = tokio::fs::read(path).await.ok()?;
    FORMATS.opens(path, &file).then_some(view)
}

async fn load(path: PathBuf, name: String) -> Result<Bytes, Text> {
    tokio::task::spawn_blocking(move || {
        let most = if page_source(&path) {
            MOST_PAGE_SOURCE
        } else {
            MOST_FILE_BYTES
        };
        let file = std::fs::File::open(&path)
            .map_err(|_| text!("{name} could not be read.", name = &name))?;
        let mut source = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(file, most + 1), &mut source)
            .map_err(|_| text!("{name} could not be read.", name = &name))?;
        if source.len() as u64 > most {
            return Err(text!(
                "{name} grew past {most} MB.",
                name = &name,
                most = most / 1024 / 1024
            ));
        }
        Ok(Bytes::from(if page_source(&path) {
            page::page(&name, &source)
        } else {
            source
        }))
    })
    .await
    .map_err(|_| text!("The canvas could not read the file."))?
}

fn page_source(path: &Path) -> bool {
    kind_of(path) == Some(CanvasKind::Page) && !is_html(path)
}

fn is_html(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "html" | "htm"))
}

pub(super) fn key_of(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| key_of(path), |name| name.to_string_lossy().into_owned())
        .chars()
        .take(NAME_CHARACTERS)
        .collect()
}
