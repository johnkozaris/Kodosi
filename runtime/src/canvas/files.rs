use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::SystemTime,
};

use base64::Engine as _;
use bytes::Bytes;

use super::{
    CanvasAnswer, CanvasItem, CanvasKind, MOST_FILE_BYTES, MOST_ITEMS, MOST_KEY, PART_BYTES,
    kind_of, page,
};

const MOST_PAGE_SOURCE: u64 = 8 * 1024 * 1024;
const NAME_CHARACTERS: usize = 128;
const SNAPSHOT_KEPT: std::time::Duration = std::time::Duration::from_mins(1);

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
}

struct Snapshot {
    key: String,
    revision: u64,
    bytes: Bytes,
    used: tokio::time::Instant,
}

pub(super) struct Shown {
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

impl Files {
    pub(super) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub(super) fn items(&self) -> Vec<CanvasItem> {
        self.items.iter().map(|item| item.listed.clone()).collect()
    }

    /// Puts each file on the canvas, or brings it to the front. The newest comes first.
    pub(super) async fn show(
        &mut self,
        paths: Vec<PathBuf>,
        stamp: &Stamp,
    ) -> Result<Showing, String> {
        let mut checked = Vec::with_capacity(paths.len());
        for path in paths {
            checked.push(examine(path).await?);
        }
        let mut shown = Vec::new();
        for (path, kind, seen, loads) in checked {
            let key = key_of(&path);
            let name = name_of(&path);
            let changed = stamp.next();
            let item = match self.items.iter().position(|item| item.listed.key == key) {
                Some(at) => {
                    let mut item = self.items.remove(at);
                    if item.seen.as_ref() != Some(&seen) {
                        item.listed.revision += 1;
                    }
                    item.listed.bytes = seen.bytes;
                    item.listed.missing = false;
                    item.listed.changed = changed;
                    item.seen = Some(seen);
                    item
                }
                None => Item {
                    listed: CanvasItem {
                        key,
                        name: name.clone(),
                        kind,
                        revision: 1,
                        bytes: seen.bytes,
                        changed,
                        missing: false,
                    },
                    path,
                    seen: Some(seen),
                },
            };
            self.items.insert(0, item);
            shown.push(Shown { name, loads });
        }
        let dropped = self
            .items
            .drain(MOST_ITEMS.min(self.items.len())..)
            .map(|item| item.listed.name)
            .collect();
        Ok(Showing { shown, dropped })
    }

    /// Brings back the files of a terminal that returns. A file that is gone shows as missing.
    pub(super) fn restore(&mut self, paths: Vec<PathBuf>, stamp: &Stamp) {
        for path in paths.into_iter().take(MOST_ITEMS) {
            let Some(kind) = kind_of(&path) else { continue };
            let key = key_of(&path);
            if self.items.iter().any(|item| item.listed.key == key) {
                continue;
            }
            self.items.push(Item {
                listed: CanvasItem {
                    key,
                    name: name_of(&path),
                    kind,
                    revision: 1,
                    bytes: 0,
                    changed: stamp.next(),
                    missing: false,
                },
                path,
                seen: None,
            });
        }
    }

    /// Checks each file for a change. A changed file gets a new revision.
    pub(super) async fn check(&mut self, stamp: &Stamp) -> bool {
        let mut any = false;
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
                        item.listed.revision += 1;
                        item.listed.changed = stamp.next();
                    }
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
        any
    }

    /// Removes the items with these keys, or all items when there are no keys.
    pub(super) fn remove(&mut self, keys: &[String]) -> usize {
        let before = self.items.len();
        self.items
            .retain(|item| !keys.is_empty() && !keys.contains(&item.listed.key));
        self.snapshot = self.snapshot.take().filter(|snapshot| {
            self.items
                .iter()
                .any(|item| item.listed.key == snapshot.key)
        });
        before - self.items.len()
    }

    /// One part of an item's content. Every part of one revision comes from one copy of the file.
    pub(super) async fn read(
        &mut self,
        key: &str,
        revision: u64,
        offset: u64,
    ) -> Result<CanvasAnswer, String> {
        let item = self
            .items
            .iter()
            .find(|item| item.listed.key == key)
            .ok_or_else(|| "This item is no longer on the canvas.".to_owned())?;
        if item.listed.missing {
            return Err(format!(
                "{} is missing. The agent can show it again.",
                item.listed.name
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
            .ok_or_else(|| "The canvas could not read the file.".to_owned())?;
        snapshot.used = tokio::time::Instant::now();
        let total = snapshot.bytes.len();
        let start = usize::try_from(offset).map_err(|_| "This part is past the end.".to_owned())?;
        if start > total || (start == total && total > 0) {
            return Err("This part is past the end.".to_owned());
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

async fn examine(path: PathBuf) -> Result<(PathBuf, CanvasKind, Seen, Option<String>), String> {
    let shown = path.display().to_string();
    let kind = kind_of(&path).ok_or_else(|| {
        format!(
            "{shown}: the canvas shows pages (HTML, Markdown, text), pictures, PDF and Office files."
        )
    })?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| format!("There is no file {shown}."))?;
    if !metadata.is_file() {
        return Err(format!("{shown} is not a file."));
    }
    let most = if page_source(&path) {
        MOST_PAGE_SOURCE
    } else {
        MOST_FILE_BYTES
    };
    if metadata.len() > most {
        return Err(format!(
            "{shown} has {} MB. The canvas takes files up to {} MB.",
            metadata.len().div_ceil(1024 * 1024),
            most / 1024 / 1024
        ));
    }
    let path = kodosi_os::fs::canonicalize(&path).unwrap_or(path);
    if key_of(&path).len() > MOST_KEY {
        return Err(format!("The path of {shown} is too long for the canvas."));
    }
    let loads = if is_html(&path) {
        tokio::fs::read(&path)
            .await
            .ok()
            .and_then(|source| page::network_reference(&source))
    } else {
        None
    };
    Ok((
        path,
        kind,
        Seen {
            bytes: metadata.len(),
            modified: metadata.modified().ok(),
        },
        loads,
    ))
}

async fn load(path: PathBuf, name: String) -> Result<Bytes, String> {
    tokio::task::spawn_blocking(move || {
        let most = if page_source(&path) {
            MOST_PAGE_SOURCE
        } else {
            MOST_FILE_BYTES
        };
        let file = std::fs::File::open(&path).map_err(|_| format!("{name} could not be read."))?;
        let mut source = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(file, most + 1), &mut source)
            .map_err(|_| format!("{name} could not be read."))?;
        if source.len() as u64 > most {
            return Err(format!("{name} grew past {} MB.", most / 1024 / 1024));
        }
        Ok(Bytes::from(if page_source(&path) {
            page::page(&name, &source)
        } else {
            source
        }))
    })
    .await
    .map_err(|_| "The canvas could not read the file.".to_owned())?
}

fn page_source(path: &Path) -> bool {
    kind_of(path) == Some(CanvasKind::Page) && !is_html(path)
}

fn is_html(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "html" | "htm"))
}

fn key_of(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| key_of(path), |name| name.to_string_lossy().into_owned())
        .chars()
        .take(NAME_CHARACTERS)
        .collect()
}
