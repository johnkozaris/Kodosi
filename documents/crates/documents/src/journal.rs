use std::{
    collections::HashMap,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use document::{Assets, Hash, Restored, Rev, Seq};
use serde::{Deserialize, Serialize};

const ORIGINAL: &str = "original";
const RECORDS: &str = "records";
const META: &str = "meta.json";
const ASSETS: &str = "assets";
const CHANGE: u8 = 0;
const SAVED: u8 = 1;
const HEADER: usize = 1 + 4;

/// What a restart needs beside the records.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    path: PathBuf,
    epoch: u64,
    first_rev: Rev,
    seqs: HashMap<String, Seq>,
}

/// A journal as a restart finds it.
pub(crate) struct Found {
    pub original: Vec<u8>,
    pub restored: Restored,
    pub records: Vec<Vec<u8>>,
    pub assets: Assets,
    pub saved_hash: Option<Hash>,
}

/// A copy of the file as it was opened, and each change since. It lives in the host's state
/// folder, only the user can read it, and a clean close deletes it.
pub(crate) struct Journal {
    folder: PathBuf,
    records: Option<std::fs::File>,
    bytes: u64,
}

fn private_folder(folder: &Path) -> io::Result<()> {
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

fn write_replacing(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("new");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_data()?;
    std::fs::rename(&temporary, path)
}

impl Journal {
    /// Starts a journal from a file, in place of any earlier one.
    pub(crate) fn start(
        folder: &Path,
        path: &Path,
        file: &[u8],
        epoch: u64,
        first_rev: Rev,
        seqs: HashMap<String, Seq>,
    ) -> io::Result<Self> {
        drop(std::fs::remove_dir_all(folder));
        private_folder(folder)?;
        private_folder(&folder.join(ASSETS))?;
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

    /// Opens the journal of an earlier run, if a usable one is there. A torn last record is a
    /// change that nobody heard of, so it goes.
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
        let mut assets = Assets::default();
        for entry in std::fs::read_dir(folder.join(ASSETS)).ok()?.flatten() {
            if let Ok(bytes) = std::fs::read(entry.path()) {
                assets.put(&bytes);
            }
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
                assets,
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

    /// Makes what was appended safe, before anyone hears of it.
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

    pub(crate) fn keep_asset(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        write_replacing(&self.folder.join(ASSETS).join(name), bytes)
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn remove(&mut self) {
        self.records = None;
        drop(std::fs::remove_dir_all(&self.folder));
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

    #[test]
    fn a_journal_of_another_file_is_not_used() {
        let state = tempfile::tempdir().unwrap();
        let folder = state.path().join("one");
        drop(Journal::start(&folder, Path::new("/a"), b"", 1, 0, HashMap::new()).unwrap());
        assert!(Journal::find(&folder, Path::new("/b")).is_none());
    }

    #[test]
    fn a_close_leaves_nothing_behind() {
        let state = tempfile::tempdir().unwrap();
        let folder = state.path().join("one");
        let mut journal =
            Journal::start(&folder, Path::new("/a"), b"x", 1, 0, HashMap::new()).unwrap();
        journal.remove();
        assert!(!folder.exists());
    }
}
