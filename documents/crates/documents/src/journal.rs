use std::{
    collections::HashMap,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use document::{Hash, Restored, Rev, Seq};
use serde::{Deserialize, Serialize};

const ORIGINAL: &str = "original";
const RECORDS: &str = "records";
const META: &str = "meta.json";
const CHANGE: u8 = 0;
const SAVED: u8 = 1;
const HEADER: usize = 1 + 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    path: PathBuf,
    epoch: u64,
    first_rev: Rev,
    seqs: HashMap<String, Seq>,
}

pub(crate) struct Found {
    pub original: Vec<u8>,
    pub restored: Restored,
    pub records: Vec<Vec<u8>>,
    pub saved_hash: Option<Hash>,
}

/// Per-document crash recovery log storing the initial file and appended changes under a
/// user-only (`0700`) state directory; removed on clean close.
pub(crate) struct Journal {
    folder: PathBuf,
    records: Option<std::fs::File>,
    bytes: u64,
}

pub(crate) fn private_folder(folder: &Path) -> io::Result<()> {
    std::fs::create_dir_all(folder)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn open_records(folder: &Path) -> io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(folder.join(RECORDS))
}

/// Remove `meta.json` first so the directory is immediately invalidated as a journal even if
/// removing the remaining files fails or crashes midway.
fn discard(folder: &Path) -> io::Result<()> {
    if let Err(problem) = std::fs::remove_file(folder.join(META))
        && problem.kind() != io::ErrorKind::NotFound
    {
        return Err(problem);
    }
    match std::fs::remove_dir_all(folder) {
        Err(problem) if problem.kind() != io::ErrorKind::NotFound => Err(problem),
        _ => Ok(()),
    }
}

pub(crate) fn write_replacing(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("new");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_data()?;
    std::fs::rename(&temporary, path)
}

impl Journal {
    pub(crate) fn start(
        folder: &Path,
        path: &Path,
        file: &[u8],
        epoch: u64,
        first_rev: Rev,
        seqs: HashMap<String, Seq>,
    ) -> io::Result<Self> {
        discard(folder)?;
        private_folder(folder)?;
        write_replacing(&folder.join(ORIGINAL), file)?;
        let meta = Meta {
            path: path.to_owned(),
            epoch,
            first_rev,
            seqs,
        };
        write_replacing(&folder.join(META), &serde_json::to_vec(&meta)?)?;
        Ok(Self {
            folder: folder.to_owned(),
            records: Some(open_records(folder)?),
            bytes: 0,
        })
    }

    /// Truncates any torn trailing record, since a change is never broadcast until synced.
    pub(crate) fn find(folder: &Path, path: &Path) -> Option<(Self, Found)> {
        let meta: Meta = serde_json::from_slice(&std::fs::read(folder.join(META)).ok()?).ok()?;
        if meta.path != path {
            return None;
        }
        let original = std::fs::read(folder.join(ORIGINAL)).ok()?;
        let bytes = std::fs::read(folder.join(RECORDS)).unwrap_or_default();
        let mut records = Vec::new();
        let mut saved = Vec::new();
        let mut saved_hash = None;
        let mut rest = bytes.as_slice();
        while let Some((&[kind, a, b, c, d], tail)) = rest.split_first_chunk::<HEADER>() {
            let length = u32::from_be_bytes([a, b, c, d]) as usize;
            let Some((body, tail)) = tail.split_at_checked(length) else {
                break;
            };
            match kind {
                CHANGE => records.push(body.to_vec()),
                SAVED => {
                    if let Some((rev, hash)) = body.split_first_chunk::<8>() {
                        saved.push(Rev::from_be_bytes(*rev));
                        saved_hash = Hash::try_from(hash).ok();
                    }
                }
                _ => return None,
            }
            rest = tail;
        }
        let whole = bytes.len() - rest.len();
        let records_file = std::fs::OpenOptions::new()
            .write(true)
            .open(folder.join(RECORDS))
            .ok();
        if let Some(file) = records_file {
            drop(file.set_len(whole as u64));
        }
        let journal = Self {
            folder: folder.to_owned(),
            records: Some(open_records(folder).ok()?),
            bytes: whole as u64,
        };
        Some((
            journal,
            Found {
                original,
                restored: Restored {
                    epoch: meta.epoch,
                    first_rev: meta.first_rev,
                    seqs: meta.seqs,
                    saved,
                },
                records,
                saved_hash,
            },
        ))
    }

    fn record(&mut self, kind: u8, body: &[u8]) -> io::Result<()> {
        let length = u32::try_from(body.len()).map_err(io::Error::other)?;
        let mut record = Vec::with_capacity(HEADER + body.len());
        record.push(kind);
        record.extend_from_slice(&length.to_be_bytes());
        record.extend_from_slice(body);
        if let Some(records) = &mut self.records {
            records.write_all(&record)?;
        }
        self.bytes += record.len() as u64;
        Ok(())
    }

    pub(crate) fn append(&mut self, change: &[u8]) -> io::Result<()> {
        self.record(CHANGE, change)
    }

    pub(crate) fn sync(&self) -> io::Result<()> {
        self.records
            .as_ref()
            .map_or(Ok(()), std::fs::File::sync_data)
    }

    pub(crate) fn saved(&mut self, rev: Rev, hash: &Hash) -> io::Result<()> {
        let mut body = rev.to_be_bytes().to_vec();
        body.extend_from_slice(hash);
        self.record(SAVED, &body)?;
        self.sync()
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn remove(&mut self) {
        self.records = None;
        if let Err(problem) = discard(&self.folder) {
            tracing::warn!(%problem, "a journal did not go away completely");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_journal_comes_back_with_its_changes_and_saves() {
        let state = tempfile::tempdir().unwrap();
        let folder = state.path().join("one");
        let path = Path::new("/files/a.docx");
        let mut journal = Journal::start(&folder, path, b"file", 7, 0, HashMap::new()).unwrap();
        journal.append(b"change one").unwrap();
        journal.saved(1, &[9; 32]).unwrap();
        journal.append(b"change two").unwrap();
        journal.sync().unwrap();
        drop(journal);

        let (_journal, found) = Journal::find(&folder, path).unwrap();
        assert_eq!(found.original, b"file");
        assert_eq!(found.restored.epoch, 7);
        assert_eq!(found.restored.saved, [1]);
        assert_eq!(found.saved_hash, Some([9; 32]));
        assert_eq!(
            found.records,
            [b"change one".to_vec(), b"change two".to_vec()]
        );
    }

    #[test]
    fn a_torn_record_goes_and_the_journal_goes_on() {
        let state = tempfile::tempdir().unwrap();
        let folder = state.path().join("one");
        let path = Path::new("/files/a.docx");
        let mut journal = Journal::start(&folder, path, b"file", 7, 0, HashMap::new()).unwrap();
        journal.append(b"whole").unwrap();
        journal.sync().unwrap();
        drop(journal);
        let records = folder.join(RECORDS);
        let mut bytes = std::fs::read(&records).unwrap();
        bytes.extend_from_slice(&[CHANGE, 0, 0, 0, 200, 1, 2]);
        std::fs::write(&records, bytes).unwrap();

        let (mut journal, found) = Journal::find(&folder, path).unwrap();
        assert_eq!(found.records, [b"whole".to_vec()]);
        journal.append(b"next").unwrap();
        journal.sync().unwrap();
        drop(journal);
        let (_journal, found) = Journal::find(&folder, path).unwrap();
        assert_eq!(found.records, [b"whole".to_vec(), b"next".to_vec()]);
    }

    /// On APFS (which iterates directory entries by name hash), `zk` sorts between `records` and
    /// the other files while `pc` sorts before all of them, letting a read-only directory simulate
    /// a partial `remove_dir_all` failure at those points.
    #[cfg(unix)]
    fn stuck(folder: &Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let stuck = folder.join(name);
        std::fs::create_dir(&stuck).unwrap();
        std::fs::write(stuck.join("left"), b"").unwrap();
        std::fs::set_permissions(&stuck, std::fs::Permissions::from_mode(0o500)).unwrap();
        stuck
    }

    #[cfg(unix)]
    fn loose(stuck: &Path) {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(stuck, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_journal_that_goes_only_in_part_is_no_journal() {
        let state = tempfile::tempdir().unwrap();
        let folder = state.path().join("one");
        let path = Path::new("/files/a.docx");
        let mut journal = Journal::start(&folder, path, b"file", 7, 0, HashMap::new()).unwrap();
        journal.append(b"change").unwrap();
        journal.sync().unwrap();
        let stuck = stuck(&folder, "zk");
        journal.remove();
        let found = Journal::find(&folder, path).is_some();
        loose(&stuck);
        assert!(!found);
    }

    #[cfg(unix)]
    #[test]
    fn a_new_journal_never_takes_the_records_of_one_that_would_not_go() {
        let state = tempfile::tempdir().unwrap();
        let folder = state.path().join("one");
        let path = Path::new("/files/a.docx");
        let mut journal = Journal::start(&folder, path, b"file", 7, 0, HashMap::new()).unwrap();
        journal.append(b"change").unwrap();
        journal.sync().unwrap();
        drop(journal);
        let stuck = stuck(&folder, "pc");
        drop(Journal::start(
            &folder,
            path,
            b"saved",
            8,
            1,
            HashMap::new(),
        ));
        let found = Journal::find(&folder, path);
        loose(&stuck);
        assert!(found.is_none_or(|(_, found)| found.records.is_empty()));
    }
}
