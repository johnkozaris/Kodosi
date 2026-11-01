use std::path::{Path, PathBuf};

use super::{Cookie, CopyError, Kind, Read, SignInSource};

const STORE: &str =
    "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies";
const FILE_MARK: &[u8] = b"cook";
const SECURE: u32 = 1;
const HTTP_ONLY: u32 = 4;
const SECONDS_TO_2001: f64 = 978_307_200.0;
const RECORD_HEAD: usize = 56;

pub(super) fn sources(home: &Path) -> Vec<SignInSource> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let folder: PathBuf = home.join(STORE);
    vec![SignInSource {
        name: "Safari".to_owned(),
        kind: Kind::Safari,
        folder,
    }]
}

fn number_le(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn number_be(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn date_at(bytes: &[u8], at: usize) -> Option<f64> {
    Some(f64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

fn text_at(record: &[u8], at: usize) -> Option<String> {
    let rest = record.get(at..)?;
    let end = rest.iter().position(|byte| *byte == 0)?;
    String::from_utf8(rest[..end].to_vec()).ok()
}

fn cookie_of(record: &[u8]) -> Option<Cookie> {
    let flags = number_le(record, 8)?;
    let offset = |at: usize| Some(number_le(record, at)? as usize);
    let (domain, name, path, value) = (offset(16)?, offset(20)?, offset(24)?, offset(28)?);
    (domain >= RECORD_HEAD).then_some(())?;
    Some(Cookie {
        domain: text_at(record, domain)?,
        name: text_at(record, name)?,
        path: text_at(record, path)?,
        value: text_at(record, value)?,
        secure: flags & SECURE != 0,
        http_only: flags & HTTP_ONLY != 0,
        same_site: None,
        expires: Some(date_at(record, 40)? + SECONDS_TO_2001),
    })
}

/// The cookies of one page of the file.
fn page_cookies(page: &[u8], read: &mut Read) -> Option<()> {
    let count = number_le(page, 4)? as usize;
    for index in 0..count {
        let start = number_le(page, 8 + index * 4)? as usize;
        let size = number_le(page, start)? as usize;
        match page.get(start..start + size).and_then(cookie_of) {
            Some(cookie) => read.cookies.push(cookie),
            None => read.skipped += 1,
        }
    }
    Some(())
}

fn parse(bytes: &[u8]) -> Option<Read> {
    bytes.starts_with(FILE_MARK).then_some(())?;
    let pages = number_be(bytes, 4)? as usize;
    let mut read = Read::default();
    let mut start = 8 + pages * 4;
    for index in 0..pages {
        let size = number_be(bytes, 8 + index * 4)? as usize;
        page_cookies(bytes.get(start..start + size)?, &mut read)?;
        start += size;
    }
    Some(read)
}

pub(super) fn read(file: &Path) -> Result<Read, CopyError> {
    let bytes = std::fs::read(file)?;
    parse(&bytes).ok_or_else(|| CopyError::Store("the file is not valid".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(domain: &str, name: &str, path: &str, value: &str, flags: u32) -> Vec<u8> {
        let strings = [domain, name, path, value];
        let mut offsets = Vec::new();
        let mut text = Vec::new();
        for string in strings {
            offsets.push(u32::try_from(RECORD_HEAD + text.len()).unwrap());
            text.extend_from_slice(string.as_bytes());
            text.push(0);
        }
        let size = u32::try_from(RECORD_HEAD + text.len()).unwrap();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        for offset in offsets {
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(&1_000_000_000.0_f64.to_le_bytes());
        bytes.extend_from_slice(&0.0_f64.to_le_bytes());
        bytes.extend_from_slice(&text);
        bytes
    }

    fn file(records: &[Vec<u8>]) -> Vec<u8> {
        let mut page = vec![0, 0, 1, 0];
        page.extend_from_slice(&u32::try_from(records.len()).unwrap().to_le_bytes());
        let mut at = 8 + records.len() * 4 + 4;
        for record in records {
            page.extend_from_slice(&u32::try_from(at).unwrap().to_le_bytes());
            at += record.len();
        }
        page.extend_from_slice(&[0; 4]);
        for record in records {
            page.extend_from_slice(record);
        }
        let mut bytes = FILE_MARK.to_vec();
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&u32::try_from(page.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(&page);
        bytes
    }

    #[test]
    fn cookies_of_a_page_read_with_their_flags_and_date() {
        let bytes = file(&[
            record(".site.test", "a", "/", "1", SECURE | HTTP_ONLY),
            record(".other.test", "b", "/x", "2", 0),
        ]);
        let read = parse(&bytes).unwrap();
        assert_eq!(read.cookies.len(), 2);
        let first = &read.cookies[0];
        assert_eq!(
            (
                first.domain.as_str(),
                first.name.as_str(),
                first.value.as_str()
            ),
            (".site.test", "a", "1")
        );
        assert!(first.secure && first.http_only);
        assert_eq!(first.expires, Some(1_000_000_000.0 + SECONDS_TO_2001));
        assert_eq!(read.cookies[1].path, "/x");
    }

    #[test]
    fn a_short_file_is_refused_and_does_not_panic() {
        let bytes = file(&[record(".site.test", "a", "/", "1", 0)]);
        assert!(parse(&bytes[..12]).is_none());
        assert!(parse(b"nope").is_none());
    }
}
