use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::AgentKind;
use crate::agent_fs::AgentFs;
use crate::{ConversationPage, TranscriptDecoder};

use super::path_safety;

const DEFAULT_PAGE_RECORDS: usize = 100;
const MAX_PAGE_RECORDS: usize = 500;
const DEFAULT_PAGE_BYTES: usize = 256 * 1024;
const MAX_PAGE_BYTES: usize = 1024 * 1024;

pub(crate) async fn read_session_conversation(
    agent: AgentKind,
    cwd: &str,
    session_id: &str,
    before_byte: Option<u64>,
    max_records: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ConversationPage, String> {
    let resolved_cwd: String = match agent {
        AgentKind::Claude => path_safety::canonicalize_user_dir("cwd", cwd)
            .map_err(|err| format!("invalid cwd: {err}"))?
            .to_string_lossy()
            .into_owned(),
        AgentKind::Copilot => String::new(),
    };

    if let Err(err) = path_safety::validate_uuid("sessionId", session_id) {
        return Err(format!("invalid sessionId: {err}"));
    }

    let fs_provider = AgentFs::from_kind(agent)
        .ok_or_else(|| format!("agent {} has no filesystem", agent.canonical()))?;
    let Some(path) = fs_provider.transcript_path(&resolved_cwd, session_id) else {
        return Err("could not resolve transcript path".to_owned());
    };
    let Some(transcript_root) = fs_provider.transcript_root() else {
        return Err("provider has no transcript root".to_owned());
    };

    let canonical_root = match transcript_root.canonicalize() {
        Ok(canonical) => canonical,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("transcript not found: {session_id}"));
        }
        Err(err) => {
            return Err(format!("failed to canonicalize transcript root: {err}"));
        }
    };
    let canonical = match path.canonicalize() {
        Ok(c) if c.starts_with(&canonical_root) => c,
        Ok(_) => return Err("transcript path escapes agent transcript root".to_owned()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("transcript not found: {session_id}"));
        }
        Err(err) => return Err(format!("failed to resolve transcript path: {err}")),
    };

    read_page(
        &canonical,
        fs_provider.transcript_decoder(),
        before_byte,
        max_records,
        max_bytes,
    )
    .await
}

pub(crate) async fn read_subagent_transcript(
    agent: AgentKind,
    path: &str,
    before_byte: Option<u64>,
    max_records: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ConversationPage, String> {
    let fs_provider = AgentFs::from_kind(agent)
        .ok_or_else(|| format!("agent {} has no filesystem", agent.canonical()))?;
    let Some(transcript_root) = fs_provider.transcript_root() else {
        return Err("provider has no transcript root".to_owned());
    };

    let raw_path: PathBuf = PathBuf::from(path);
    if raw_path.as_os_str().is_empty() {
        return Err("invalid input: path (empty)".to_owned());
    }
    if !raw_path.is_absolute() {
        return Err("invalid input: path (must be absolute)".to_owned());
    }

    let canonical_root = transcript_root.canonicalize().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            format!("transcript root not found: {}", transcript_root.display())
        } else {
            format!("failed to canonicalize transcript root: {err}")
        }
    })?;

    let canonical: &Path = &raw_path.canonicalize().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            format!("transcript not found: {path}")
        } else {
            format!("failed to resolve transcript path: {err}")
        }
    })?;
    if !canonical.starts_with(&canonical_root) {
        return Err("transcript path escapes agent transcript root".to_owned());
    }

    if canonical.extension().and_then(|e| e.to_str()) != Some("jsonl") {
        return Err("invalid input: path (must point to a .jsonl file)".to_owned());
    }

    read_page(
        canonical,
        fs_provider.transcript_decoder(),
        before_byte,
        max_records,
        max_bytes,
    )
    .await
}

async fn read_page(
    path: &Path,
    decoder: &'static dyn TranscriptDecoder,
    before_byte: Option<u64>,
    max_records: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ConversationPage, String> {
    let max_records = max_records
        .unwrap_or(DEFAULT_PAGE_RECORDS)
        .clamp(1, MAX_PAGE_RECORDS);
    let max_bytes = max_bytes
        .unwrap_or(DEFAULT_PAGE_BYTES)
        .clamp(1, MAX_PAGE_BYTES);
    let source_file_bytes = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("failed to inspect transcript: {error}"))?
        .len();
    let before_byte = before_byte.unwrap_or(source_file_bytes);
    if before_byte > source_file_bytes {
        return Err("transcript changed while paging; restart from the newest page".to_owned());
    }
    if before_byte == 0 {
        return Ok(ConversationPage {
            entries: Vec::new(),
            next_before_byte: None,
            source_file_bytes,
            read_bytes: 0,
            source_records: 0,
            degraded_reason: None,
        });
    }

    let read_bytes = before_byte.min(max_bytes as u64);
    let window_start = before_byte - read_bytes;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| format!("failed to open transcript: {error}"))?;
    let window_starts_at_record_boundary = if window_start == 0 {
        true
    } else {
        file.seek(std::io::SeekFrom::Start(window_start - 1))
            .await
            .map_err(|error| format!("failed to seek transcript: {error}"))?;
        let mut previous_byte = [0_u8; 1];
        file.read_exact(&mut previous_byte)
            .await
            .map_err(|error| format!("failed to inspect transcript boundary: {error}"))?;
        previous_byte[0] == b'\n'
    };
    file.seek(std::io::SeekFrom::Start(window_start))
        .await
        .map_err(|error| format!("failed to seek transcript: {error}"))?;
    let mut window = vec![
        0;
        usize::try_from(read_bytes)
            .map_err(|_| "transcript page size is unsupported".to_owned())?
    ];
    file.read_exact(&mut window)
        .await
        .map_err(|error| format!("failed to read transcript page: {error}"))?;

    let aligned_start = if window_starts_at_record_boundary {
        0
    } else if let Some(newline) = window.iter().position(|byte| *byte == b'\n') {
        newline + 1
    } else {
        return Ok(ConversationPage {
            entries: Vec::new(),
            next_before_byte: Some(window_start),
            source_file_bytes,
            read_bytes,
            source_records: 0,
            degraded_reason: Some(
                "A transcript record exceeds this page's byte limit; continue loading earlier pages"
                    .to_owned(),
            ),
        });
    };

    let aligned = &window[aligned_start..];
    if aligned.is_empty() {
        return Ok(ConversationPage {
            entries: Vec::new(),
            next_before_byte: (window_start > 0).then_some(window_start),
            source_file_bytes,
            read_bytes,
            source_records: 0,
            degraded_reason: None,
        });
    }

    let last_newline = aligned.iter().rposition(|byte| *byte == b'\n');
    let valid_unterminated_eof = before_byte == source_file_bytes
        && last_newline.is_none_or(|newline| newline + 1 < aligned.len())
        && serde_json::from_slice::<serde_json::Value>(
            last_newline.map_or(aligned, |newline| &aligned[newline + 1..]),
        )
        .is_ok();
    let complete_records = if valid_unterminated_eof {
        aligned
    } else if let Some(last_newline) = last_newline {
        &aligned[..=last_newline]
    } else {
        return Ok(ConversationPage {
            entries: Vec::new(),
            next_before_byte: (window_start > 0).then_some(window_start),
            source_file_bytes,
            read_bytes,
            source_records: 0,
            degraded_reason: (window_start > 0).then(|| {
                "A transcript record exceeds this page's byte limit; continue loading earlier pages"
                    .to_owned()
            }),
        });
    };
    let mut record_starts = vec![0_usize];
    for (index, byte) in complete_records.iter().enumerate() {
        if *byte == b'\n' && index + 1 < complete_records.len() {
            record_starts.push(index + 1);
        }
    }
    let selected_record = record_starts.len().saturating_sub(max_records);
    let selected_start = record_starts[selected_record];
    let absolute_start = window_start
        + u64::try_from(aligned_start + selected_start)
            .map_err(|_| "transcript cursor overflow".to_owned())?;
    let raw = std::str::from_utf8(&complete_records[selected_start..])
        .map_err(|error| format!("transcript page is not UTF-8: {error}"))?;
    let entries = decoder
        .decode_conversation(raw)
        .map_err(|error| format!("failed to decode transcript page: {error}"))?;

    Ok(ConversationPage {
        entries,
        next_before_byte: (absolute_start > 0).then_some(absolute_start),
        source_file_bytes,
        read_bytes,
        source_records: record_starts.len() - selected_record,
        degraded_reason: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};

    #[tokio::test]
    async fn rejects_empty_path() {
        let result = read_subagent_transcript(AgentKind::Claude, "", None, None, None).await;
        std::assert_matches!(result, Err(e) if e.contains("empty"));
    }

    #[tokio::test]
    async fn rejects_relative_path() {
        let result = read_subagent_transcript(
            AgentKind::Claude,
            "subagents/agent-X.jsonl",
            None,
            None,
            None,
        )
        .await;
        std::assert_matches!(result, Err(e) if e.contains("must be absolute"));
    }

    #[tokio::test]
    async fn rejects_path_outside_transcript_root() {
        let result =
            read_subagent_transcript(AgentKind::Claude, "/etc/passwd", None, None, None).await;
        assert!(result.is_err(), "must reject /etc/passwd");
    }

    #[tokio::test]
    async fn rejects_non_jsonl_extension() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
        let bogus = format!("{home}/.claude/projects/x/settings.json");
        let result = read_subagent_transcript(AgentKind::Claude, &bogus, None, None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn pages_backwards_without_rereading_the_whole_transcript() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        std::fs::write(
            &path,
            concat!(
                "{\"type\":\"user\",\"message\":{\"content\":\"first\"}}\n",
                "{\"type\":\"user\",\"message\":{\"content\":\"second\"}}\n",
                "{\"type\":\"user\",\"message\":{\"content\":\"third\"}}\n"
            ),
        )
        .expect("fixture");

        let newest = read_page(
            &path,
            crate::claude::transcript::ClaudeTranscriptDecoder::INSTANCE,
            None,
            Some(1),
            Some(4096),
        )
        .await
        .expect("newest page");
        assert_eq!(newest.entries.len(), 1);
        assert_eq!(newest.entries[0].content, "third");
        let second = read_page(
            &path,
            crate::claude::transcript::ClaudeTranscriptDecoder::INSTANCE,
            newest.next_before_byte,
            Some(1),
            Some(4096),
        )
        .await
        .expect("second page");
        assert_eq!(second.entries.len(), 1);
        assert_eq!(second.entries[0].content, "second");
        let first = read_page(
            &path,
            crate::claude::transcript::ClaudeTranscriptDecoder::INSTANCE,
            second.next_before_byte,
            Some(1),
            Some(4096),
        )
        .await
        .expect("first page");
        assert_eq!(first.entries.len(), 1);
        assert_eq!(first.entries[0].content, "first");
        assert!(first.next_before_byte.is_none());
    }

    #[tokio::test]
    async fn keeps_record_when_window_starts_exactly_at_its_boundary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        let first = "{\"type\":\"user\",\"message\":{\"content\":\"first\"}}\n";
        let second = "{\"type\":\"user\",\"message\":{\"content\":\"second\"}}\n";
        std::fs::write(&path, format!("{first}{second}")).expect("fixture");

        let page = read_page(
            &path,
            crate::claude::transcript::ClaudeTranscriptDecoder::INSTANCE,
            None,
            Some(1),
            Some(second.len()),
        )
        .await
        .expect("boundary page");

        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].content, "second");
        assert_eq!(page.source_records, 1);
        assert_eq!(page.next_before_byte, Some(first.len() as u64));
    }

    #[tokio::test]
    async fn incomplete_tail_does_not_hide_last_complete_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        let first = "{\"type\":\"user\",\"message\":{\"content\":\"first\"}}\n";
        let second = "{\"type\":\"user\",\"message\":{\"content\":\"second\"}}\n";
        let live_tail = "{\"type\":\"user\",\"message\":{\"content\":\"live tail";
        std::fs::write(&path, format!("{first}{second}{live_tail}")).expect("fixture");

        let page = read_page(
            &path,
            crate::claude::transcript::ClaudeTranscriptDecoder::INSTANCE,
            None,
            Some(1),
            Some(4096),
        )
        .await
        .expect("page with live tail");

        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].content, "second");
        assert_eq!(page.source_records, 1);
        assert_eq!(page.next_before_byte, Some(first.len() as u64));
    }

    #[tokio::test]
    async fn valid_unterminated_eof_record_is_included() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        let record = "{\"type\":\"user\",\"message\":{\"content\":\"complete eof\"}}";
        std::fs::write(&path, record).expect("fixture");

        let page = read_page(
            &path,
            crate::claude::transcript::ClaudeTranscriptDecoder::INSTANCE,
            None,
            Some(1),
            Some(4096),
        )
        .await
        .expect("unterminated complete record");

        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].content, "complete eof");
        assert_eq!(page.source_records, 1);
        assert!(page.next_before_byte.is_none());
    }

    #[tokio::test]
    async fn oversized_record_advances_by_a_bounded_window() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        std::fs::write(&path, vec![b'x'; 4096]).expect("fixture");

        let page = read_page(
            &path,
            crate::claude::transcript::ClaudeTranscriptDecoder::INSTANCE,
            None,
            Some(100),
            Some(128),
        )
        .await
        .expect("bounded page");
        assert!(page.entries.is_empty());
        assert_eq!(page.read_bytes, 128);
        assert_eq!(page.next_before_byte, Some(4096 - 128));
        assert!(page.degraded_reason.is_some());
    }

    #[tokio::test]
    async fn sparse_multi_gigabyte_transcript_reads_only_the_tail_window() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        let mut file = std::fs::File::create(&path).expect("fixture");
        file.seek(SeekFrom::Start(2 * 1024 * 1024 * 1024 - 1))
            .expect("sparse seek");
        file.write_all(
            concat!(
                "\n",
                "{\"type\":\"user\",\"message\":{\"content\":\"tail one\"}}\n",
                "{\"type\":\"user\",\"message\":{\"content\":\"tail two\"}}\n"
            )
            .as_bytes(),
        )
        .expect("tail records");

        let page = read_page(
            &path,
            crate::claude::transcript::ClaudeTranscriptDecoder::INSTANCE,
            None,
            Some(10),
            Some(4096),
        )
        .await
        .expect("sparse page");
        assert!(page.source_file_bytes > 2 * 1024 * 1024 * 1024);
        assert_eq!(page.read_bytes, 4096);
        assert_eq!(
            page.entries
                .iter()
                .map(|entry| entry.content.as_str())
                .collect::<Vec<_>>(),
            vec!["tail one", "tail two"]
        );
    }
}
