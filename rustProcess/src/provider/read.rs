use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

use super::{ConversationPage, Provider, decode, storage};

pub(super) fn page(
    mut file: File,
    provider: Provider,
    before_byte: Option<u64>,
    limit: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ConversationPage, String> {
    let limit = storage::bounded("limit", limit.unwrap_or(100), 500)?;
    let max_bytes = storage::bounded("maxBytes", max_bytes.unwrap_or(256 * 1024), 1024 * 1024)?;
    let source_file_bytes = file
        .metadata()
        .map_err(|error| format!("Could not inspect transcript: {error}"))?
        .len();
    let before_byte = before_byte.unwrap_or(source_file_bytes);
    if before_byte > source_file_bytes {
        return Err("Transcript changed while paging; refresh from the newest page".to_owned());
    }
    let read_bytes = before_byte.min(max_bytes as u64);
    let mut page = ConversationPage {
        entries: Vec::new(),
        next_before_byte: None,
        source_file_bytes,
        read_bytes,
        source_records: 0,
        degraded_reason: None,
    };
    if before_byte == 0 {
        return Ok(page);
    }
    let window_start = before_byte - read_bytes;
    let starts_at_boundary = if window_start == 0 {
        true
    } else {
        file.seek(SeekFrom::Start(window_start - 1))
            .map_err(|error| error.to_string())?;
        let mut previous = [0];
        file.read_exact(&mut previous)
            .map_err(|error| error.to_string())?;
        previous[0] == b'\n'
    };
    file.seek(SeekFrom::Start(window_start))
        .map_err(|error| error.to_string())?;
    let mut window = vec![
        0;
        usize::try_from(read_bytes)
            .map_err(|_| "Transcript page size unsupported".to_owned())?
    ];
    file.read_exact(&mut window)
        .map_err(|error| format!("Could not read transcript page: {error}"))?;
    let aligned_start = if starts_at_boundary {
        0
    } else if let Some(newline) = window.iter().position(|byte| *byte == b'\n') {
        newline + 1
    } else {
        page.next_before_byte = Some(window_start);
        page.degraded_reason = Some(
            "A transcript record exceeds this page's byte limit; load an earlier page".to_owned(),
        );
        return Ok(page);
    };
    let aligned = &window[aligned_start..];
    let last_newline = aligned.iter().rposition(|byte| *byte == b'\n');
    let valid_unterminated_eof = before_byte == source_file_bytes
        && last_newline.is_none_or(|newline| newline + 1 < aligned.len())
        && serde_json::from_slice::<serde_json::Value>(
            last_newline.map_or(aligned, |newline| &aligned[newline + 1..]),
        )
        .is_ok();
    let complete = if valid_unterminated_eof {
        aligned
    } else if let Some(newline) = last_newline {
        &aligned[..=newline]
    } else {
        page.next_before_byte = (window_start > 0).then_some(window_start);
        if window_start > 0 {
            page.degraded_reason = Some(
                "A transcript record exceeds this page's byte limit; load an earlier page"
                    .to_owned(),
            );
        }
        return Ok(page);
    };
    if complete.is_empty() {
        page.next_before_byte = (window_start > 0).then_some(window_start);
        return Ok(page);
    }
    let mut record_starts = vec![0];
    for (index, byte) in complete.iter().enumerate() {
        if *byte == b'\n' && index + 1 < complete.len() {
            record_starts.push(index + 1);
        }
    }
    let selected = record_starts.len().saturating_sub(limit);
    let start = record_starts[selected];
    let absolute_start = window_start + (aligned_start + start) as u64;
    let raw = std::str::from_utf8(&complete[start..])
        .map_err(|error| format!("Transcript page is not UTF-8: {error}"))?;
    page.entries = decode::conversation(provider, raw)?;
    page.source_records = record_starts.len() - selected;
    page.next_before_byte = (absolute_start > 0).then_some(absolute_start);
    Ok(page)
}
