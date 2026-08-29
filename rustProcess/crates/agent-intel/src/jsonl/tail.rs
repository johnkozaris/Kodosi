use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum TailRead {
    Missing,

    Truncated,

    Progress,
    Lines(Vec<String>),
}

#[derive(Debug, Default)]
pub struct JsonlTail {
    path: Option<PathBuf>,
    offset: u64,
    discarding_oversized_line: bool,
    has_more_complete_data: bool,
    file_identity: Option<FileIdentity>,
    prefix: Vec<u8>,
    anchor_start: u64,
    anchor: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

const FILE_FINGERPRINT_BYTES: usize = 4 * 1024;
const MAX_JSONL_LINE_BYTES: usize = 1024 * 1024;
const MAX_JSONL_BYTES_PER_READ: usize = 4 * 1024 * 1024;
const MAX_JSONL_RECORDS_PER_READ: usize = 4096;
const DISCARD_CHUNK_BYTES: usize = 64 * 1024;

impl JsonlTail {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            path: None,
            offset: 0,
            discarding_oversized_line: false,
            has_more_complete_data: false,
            file_identity: None,
            prefix: Vec::new(),
            anchor_start: 0,
            anchor: Vec::new(),
        }
    }

    pub fn set_path(&mut self, path: PathBuf) {
        if self.path.as_deref() != Some(path.as_path()) {
            self.path = Some(path);
            self.offset = 0;
            self.discarding_oversized_line = false;
            self.has_more_complete_data = false;
            self.file_identity = None;
            self.prefix.clear();
            self.anchor_start = 0;
            self.anchor.clear();
        }
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn reset_offset(&mut self) {
        self.offset = 0;
        self.discarding_oversized_line = false;
        self.has_more_complete_data = false;
        self.file_identity = None;
        self.prefix.clear();
        self.anchor_start = 0;
        self.anchor.clear();
    }

    #[must_use]
    pub const fn has_pending_drain(&self) -> bool {
        self.has_more_complete_data
    }

    pub fn read_new(&mut self) -> TailRead {
        self.has_more_complete_data = false;
        let Some(path) = self.path.as_deref() else {
            return TailRead::Missing;
        };

        let Ok(mut file) = File::open(path) else {
            return TailRead::Missing;
        };
        let Ok(metadata) = file.metadata() else {
            return TailRead::Missing;
        };
        let file_len = metadata.len();
        let identity = file_identity(&metadata);

        if self
            .file_identity
            .is_some_and(|previous| previous != identity)
            || !self.fingerprints_match(&mut file)
            || file_len < self.offset
        {
            self.reset_for_replacement(identity);
            return TailRead::Truncated;
        }
        self.file_identity = Some(identity);

        if self.offset >= file_len {
            return TailRead::Lines(Vec::new());
        }

        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(self.offset)).is_err() {
            return TailRead::Missing;
        }

        let starting_offset = self.offset;
        let mut lines = Vec::new();
        let mut bytes_this_pass = 0_usize;
        let mut partial_tail = false;
        while bytes_this_pass < MAX_JSONL_BYTES_PER_READ && lines.len() < MAX_JSONL_RECORDS_PER_READ
        {
            if self.discarding_oversized_line {
                let budget = (MAX_JSONL_BYTES_PER_READ - bytes_this_pass).min(DISCARD_CHUNK_BYTES);
                let mut discarded = Vec::new();
                let Ok(bytes_read) = Read::by_ref(&mut reader)
                    .take(u64::try_from(budget).unwrap_or(u64::MAX))
                    .read_until(b'\n', &mut discarded)
                else {
                    break;
                };
                if bytes_read == 0 {
                    break;
                }
                self.offset = self
                    .offset
                    .saturating_add(u64::try_from(bytes_read).unwrap_or(u64::MAX));
                bytes_this_pass = bytes_this_pass.saturating_add(bytes_read);
                if discarded.ends_with(b"\n") {
                    self.discarding_oversized_line = false;
                }
                continue;
            }

            let line_start = self.offset;
            let budget = MAX_JSONL_BYTES_PER_READ - bytes_this_pass;
            let limit = budget.min(MAX_JSONL_LINE_BYTES.saturating_add(1));
            let mut line_buf = Vec::new();
            let Ok(bytes_read) = Read::by_ref(&mut reader)
                .take(u64::try_from(limit).unwrap_or(u64::MAX))
                .read_until(b'\n', &mut line_buf)
            else {
                break;
            };
            if bytes_read == 0 {
                break;
            }
            bytes_this_pass = bytes_this_pass.saturating_add(bytes_read);
            if line_buf.len() > MAX_JSONL_LINE_BYTES {
                self.offset =
                    line_start.saturating_add(u64::try_from(bytes_read).unwrap_or(u64::MAX));
                self.discarding_oversized_line = !line_buf.ends_with(b"\n");
                continue;
            }
            if !line_buf.ends_with(b"\n") {
                self.offset = line_start;
                partial_tail = true;
                break;
            }
            self.offset = line_start.saturating_add(u64::try_from(bytes_read).unwrap_or(u64::MAX));
            let Ok(text) = std::str::from_utf8(&line_buf) else {
                continue;
            };
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                lines.push(trimmed.to_owned());
            }
        }

        self.capture_fingerprints(&mut reader);
        self.has_more_complete_data = self.offset < file_len && !partial_tail;
        if lines.is_empty() && self.offset > starting_offset {
            TailRead::Progress
        } else {
            TailRead::Lines(lines)
        }
    }

    fn reset_for_replacement(&mut self, identity: FileIdentity) {
        self.offset = 0;
        self.discarding_oversized_line = false;
        self.has_more_complete_data = false;
        self.file_identity = Some(identity);
        self.prefix.clear();
        self.anchor_start = 0;
        self.anchor.clear();
    }

    fn fingerprints_match(&self, file: &mut File) -> bool {
        read_exact_at(file, 0, &self.prefix) && read_exact_at(file, self.anchor_start, &self.anchor)
    }

    fn capture_fingerprints(&mut self, reader: &mut BufReader<File>) {
        let file = reader.get_mut();
        let prefix_len = usize::try_from(self.offset)
            .unwrap_or(usize::MAX)
            .min(FILE_FINGERPRINT_BYTES);
        self.prefix = read_at(file, 0, prefix_len);
        let anchor_len = usize::try_from(self.offset)
            .unwrap_or(usize::MAX)
            .min(FILE_FINGERPRINT_BYTES);
        self.anchor_start = self
            .offset
            .saturating_sub(u64::try_from(anchor_len).unwrap_or(u64::MAX));
        self.anchor = read_at(file, self.anchor_start, anchor_len);
    }
}

fn read_at(file: &mut File, offset: u64, length: usize) -> Vec<u8> {
    if length == 0 || file.seek(SeekFrom::Start(offset)).is_err() {
        return Vec::new();
    }
    let mut bytes = vec![0; length];
    match file.read_exact(&mut bytes) {
        Ok(()) => bytes,
        Err(_) => Vec::new(),
    }
}

fn read_exact_at(file: &mut File, offset: u64, expected: &[u8]) -> bool {
    expected.is_empty() || read_at(file, offset, expected.len()) == expected
}

#[cfg(unix)]
fn file_identity(metadata: &std::fs::Metadata) -> FileIdentity {
    use std::os::unix::fs::MetadataExt as _;
    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

#[cfg(not(unix))]
const fn file_identity(_metadata: &std::fs::Metadata) -> FileIdentity {
    FileIdentity {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fmt::Write as FmtWrite, io::Write as IoWrite};

    fn write_lines(path: &Path, lines: &[&str]) {
        let mut f = File::create(path).expect("create");
        for line in lines {
            writeln!(f, "{line}").expect("write");
        }
    }

    #[test]
    fn returns_missing_when_path_unset() {
        let mut tail = JsonlTail::new();
        std::assert_matches!(tail.read_new(), TailRead::Missing);
    }

    #[test]
    fn returns_empty_when_no_new_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("t.jsonl");
        File::create(&path).expect("create");
        let mut tail = JsonlTail::new();
        tail.set_path(path);
        let TailRead::Lines(lines) = tail.read_new() else {
            panic!("expected Lines");
        };
        assert!(lines.is_empty());
    }

    #[test]
    fn oversized_line_is_skipped_without_losing_following_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tail.jsonl");
        let mut body = vec![b'x'; MAX_JSONL_LINE_BYTES + 10];
        body.push(b'\n');
        body.extend_from_slice(b"{\"ok\":true}\n");
        std::fs::write(&path, body).unwrap();
        let mut tail = JsonlTail::new();
        tail.set_path(path);
        let TailRead::Lines(lines) = tail.read_new() else {
            panic!("expected lines");
        };
        assert_eq!(lines, vec!["{\"ok\":true}"]);
    }

    #[test]
    fn oversized_line_spanning_page_reports_progress_then_reaches_following_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tail.jsonl");
        let mut body = vec![b'x'; MAX_JSONL_BYTES_PER_READ + 10];
        body.push(b'\n');
        body.extend_from_slice(b"{\"after\":true}\n");
        std::fs::write(&path, body).unwrap();
        let mut tail = JsonlTail::new();
        tail.set_path(path);

        std::assert_matches!(tail.read_new(), TailRead::Progress);
        let TailRead::Lines(lines) = tail.read_new() else {
            panic!("expected following record");
        };
        assert_eq!(lines, vec!["{\"after\":true}"]);
    }

    #[test]
    fn record_budget_preserves_remaining_cursor_for_next_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tail.jsonl");
        let mut body = String::new();
        for index in 0..MAX_JSONL_RECORDS_PER_READ + 2 {
            writeln!(&mut body, "{{\"i\":{index}}}").unwrap();
        }
        std::fs::write(&path, body).unwrap();
        let mut tail = JsonlTail::new();
        tail.set_path(path);
        let TailRead::Lines(first) = tail.read_new() else {
            panic!("expected first page");
        };
        let TailRead::Lines(second) = tail.read_new() else {
            panic!("expected second page");
        };
        assert_eq!(first.len(), MAX_JSONL_RECORDS_PER_READ);
        assert_eq!(second.len(), 2);
    }

    #[test]
    fn reads_complete_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("t.jsonl");
        write_lines(&path, &[r#"{"a":1}"#, r#"{"a":2}"#]);
        let mut tail = JsonlTail::new();
        tail.set_path(path);
        let TailRead::Lines(lines) = tail.read_new() else {
            panic!("expected Lines");
        };
        assert_eq!(
            lines,
            vec![r#"{"a":1}"#.to_owned(), r#"{"a":2}"#.to_owned()]
        );
    }

    #[test]
    fn rolls_back_partial_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("t.jsonl");
        let mut f = File::create(&path).expect("create");
        f.write_all(b"{\"complete\":true}\n{\"partial")
            .expect("write");
        drop(f);
        let mut tail = JsonlTail::new();
        tail.set_path(path.clone());
        let TailRead::Lines(lines) = tail.read_new() else {
            panic!("expected Lines");
        };
        assert_eq!(lines, vec![r#"{"complete":true}"#.to_owned()]);
        let mut f = File::options()
            .append(true)
            .open(&path)
            .expect("open append");
        f.write_all(b"\":1}\n").expect("write");
        drop(f);
        let TailRead::Lines(lines) = tail.read_new() else {
            panic!("expected Lines");
        };
        assert_eq!(lines, vec![r#"{"partial":1}"#.to_owned()]);
    }

    #[test]
    fn detects_truncation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("t.jsonl");
        write_lines(&path, &[r#"{"a":1}"#, r#"{"a":2}"#, r#"{"a":3}"#]);
        let mut tail = JsonlTail::new();
        tail.set_path(path.clone());
        let TailRead::Lines(lines) = tail.read_new() else {
            panic!("expected Lines");
        };
        assert_eq!(lines.len(), 3);
        write_lines(&path, &[r#"{"b":1}"#]);
        let TailRead::Truncated = tail.read_new() else {
            panic!("expected Truncated");
        };
        let TailRead::Lines(lines) = tail.read_new() else {
            panic!("expected Lines after truncation reset");
        };
        assert_eq!(lines, vec![r#"{"b":1}"#.to_owned()]);
    }

    #[test]
    fn detects_same_inode_truncate_and_regrow_past_cursor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("t.jsonl");
        write_lines(&path, &[r#"{"old":1}"#, r#"{"old":2}"#]);
        let mut tail = JsonlTail::new();
        tail.set_path(path.clone());
        std::assert_matches!(tail.read_new(), TailRead::Lines(_));

        let mut file = File::options()
            .write(true)
            .truncate(true)
            .open(&path)
            .expect("truncate in place");
        for index in 0..8 {
            writeln!(file, "{{\"new\":{index}}}").expect("regrow");
        }
        drop(file);

        std::assert_matches!(tail.read_new(), TailRead::Truncated);
        let TailRead::Lines(lines) = tail.read_new() else {
            panic!("expected replacement replay");
        };
        assert_eq!(lines.len(), 8);
        assert_eq!(lines[0], r#"{"new":0}"#);
    }

    #[cfg(unix)]
    #[test]
    fn detects_atomic_replacement_even_when_prefix_matches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("t.jsonl");
        write_lines(&path, &[r#"{"same":1}"#, r#"{"old":2}"#]);
        let mut tail = JsonlTail::new();
        tail.set_path(path.clone());
        std::assert_matches!(tail.read_new(), TailRead::Lines(_));

        let replacement = dir.path().join("replacement.jsonl");
        write_lines(
            &replacement,
            &[r#"{"same":1}"#, r#"{"new":2}"#, r#"{"new":3}"#],
        );
        std::fs::rename(replacement, &path).expect("atomic replace");

        std::assert_matches!(tail.read_new(), TailRead::Truncated);
    }

    #[test]
    fn skips_blank_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("t.jsonl");
        let mut f = File::create(&path).expect("create");
        f.write_all(b"{\"a\":1}\n\n{\"a\":2}\n").expect("write");
        drop(f);
        let mut tail = JsonlTail::new();
        tail.set_path(path);
        let TailRead::Lines(lines) = tail.read_new() else {
            panic!("expected Lines");
        };
        assert_eq!(
            lines,
            vec![r#"{"a":1}"#.to_owned(), r#"{"a":2}"#.to_owned()]
        );
    }
}
