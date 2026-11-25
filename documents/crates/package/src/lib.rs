//! Safe OOXML ZIP package reader and patcher.
//!
//! Reading enforces entry-count, uncompressed-size, and compression-ratio limits, validates entry
//! paths, and rejects ambiguous archives (leading data, hidden entries, local/central header
//! mismatches, Info-ZIP Unicode Path aliases, or case-colliding names) that another parser could
//! interpret differently. Writing copies unmodified entries verbatim without recompressing.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::{Cursor, Read as _, Write as _},
};

use zip::{CompressionMethod, ExtraField, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MOST_ENTRIES: usize = 16 * 1024;
const MOST_UNPACKED: u64 = 512 * 1024 * 1024;
const MOST_RATIO: u64 = 1000;
const RATIO_FROM: u64 = 1024 * 1024;

const TIME_IN_LOCAL_HEADER: usize = 10;
const NAME_LENGTH_IN_LOCAL_HEADER: usize = 26;
const EXTRA_LENGTH_IN_LOCAL_HEADER: usize = 28;
const NAME_IN_LOCAL_HEADER: usize = 30;
const TIME_IN_CENTRAL_HEADER: usize = 12;
const TIME_BYTES: usize = 4;
const EXTRA_FIELD_HEADER_BYTES: usize = 4;
const EXTENDED_TIMESTAMP: u16 = 0x5455;
const UNREAD_EXTRA_FIELD: u16 = 0xffff;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackageError {
    #[error("The file is not a package: {0}")]
    Broken(String),
    #[error("The file is not safe to open: {0}")]
    Unsafe(String),
}

fn broken(error: impl std::fmt::Display) -> PackageError {
    PackageError::Broken(error.to_string())
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains('\0')
        && name.split('/').all(|part| part != "..")
}

fn u16_at(file: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(file.get(at..at + 2)?.try_into().ok()?))
}

/// Word and `SharePoint` can write mismatched local/central timestamps, and Hancell writes
/// local extended timestamp fields (`0x5455`) that the `zip` crate cannot read. Normalize
/// those harmless timestamp fields on a scratch copy before running header consistency checks.
fn without_local_time(file: &[u8], local: usize, central: usize, copy: &mut Option<Vec<u8>>) {
    let time = local + TIME_IN_LOCAL_HEADER..local + TIME_IN_LOCAL_HEADER + TIME_BYTES;
    let central_time =
        central + TIME_IN_CENTRAL_HEADER..central + TIME_IN_CENTRAL_HEADER + TIME_BYTES;
    if let (Some(local_time), Some(central_time)) = (file.get(time.clone()), file.get(central_time))
        && local_time != central_time
    {
        copy.get_or_insert_with(|| file.to_vec())[time].copy_from_slice(central_time);
    }
    let (Some(name), Some(extra)) = (
        u16_at(file, local + NAME_LENGTH_IN_LOCAL_HEADER),
        u16_at(file, local + EXTRA_LENGTH_IN_LOCAL_HEADER),
    ) else {
        return;
    };
    let mut at = local + NAME_IN_LOCAL_HEADER + usize::from(name);
    let end = at + usize::from(extra);
    while at + EXTRA_FIELD_HEADER_BYTES <= end
        && let (Some(field), Some(length)) = (u16_at(file, at), u16_at(file, at + 2))
    {
        if field == EXTENDED_TIMESTAMP {
            copy.get_or_insert_with(|| file.to_vec())[at..at + 2]
                .copy_from_slice(&UNREAD_EXTRA_FIELD.to_le_bytes());
        }
        at += EXTRA_FIELD_HEADER_BYTES + usize::from(length);
    }
}

pub struct Package<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
    names: HashSet<String>,
}

impl<'a> Package<'a> {
    pub fn open(file: &'a [u8]) -> Result<Self, PackageError> {
        let mut archive = ZipArchive::new(Cursor::new(file)).map_err(broken)?;
        if archive.len() > MOST_ENTRIES {
            return Err(PackageError::Unsafe("too many entries".to_owned()));
        }
        let mut start = archive.central_directory_start();
        let mut copy = None;
        let mut unpacked = 0_u64;
        let mut names = HashSet::new();
        let mut folded = HashSet::new();
        for index in 0..archive.len() {
            let entry = archive.by_index_raw(index).map_err(broken)?;
            let name = entry.name().map_err(broken)?.into_owned();
            if !safe_name(&name) {
                return Err(PackageError::Unsafe(format!("the entry name {name}")));
            }
            if entry
                .extra_data_fields()
                .any(|field| matches!(field, ExtraField::UnicodePath(_)))
            {
                return Err(PackageError::Unsafe(format!("a second name of {name}")));
            }
            if !folded.insert(name.to_ascii_lowercase()) {
                return Err(PackageError::Unsafe(format!("two entries named {name}")));
            }
            unpacked = unpacked.saturating_add(entry.size());
            if unpacked > MOST_UNPACKED {
                return Err(PackageError::Unsafe("too much unpacked data".to_owned()));
            }
            if entry.size() > RATIO_FROM
                && entry.size() / entry.compressed_size().max(1) > MOST_RATIO
            {
                return Err(PackageError::Unsafe(format!("the ratio of {name}")));
            }
            start = start.min(entry.header_start());
            let local = usize::try_from(entry.header_start()).map_err(broken)?;
            let central = usize::try_from(entry.central_header_start()).map_err(broken)?;
            without_local_time(file, local, central, &mut copy);
            names.insert(name);
        }
        if start != 0 {
            return Err(PackageError::Unsafe("data before the package".to_owned()));
        }
        copy.map_or_else(
            || archive.validate_header_consistency(),
            |copy| {
                ZipArchive::new(Cursor::new(copy.as_slice()))
                    .and_then(|mut copy| copy.validate_header_consistency())
            },
        )
        .map_err(|error| PackageError::Unsafe(error.to_string()))?;
        Ok(Self { archive, names })
    }

    pub fn has(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(String::as_str)
    }

    pub fn bytes(&mut self, name: &str) -> Option<Vec<u8>> {
        let entry = self.archive.by_name(name).ok()?;
        let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
        entry.take(MOST_UNPACKED).read_to_end(&mut bytes).ok()?;
        Some(bytes)
    }

    pub fn text(&mut self, name: &str) -> Option<String> {
        String::from_utf8(self.bytes(name)?).ok()
    }

    pub fn relations(&mut self, part: &str) -> HashMap<String, Relation> {
        let (folder, file) = part.rsplit_once('/').unwrap_or(("", part));
        let name = format!("{folder}/_rels/{file}.rels");
        let Some(text) = self.text(name.trim_start_matches('/')) else {
            return HashMap::new();
        };
        let Ok(rels) = roxmltree::Document::parse(&text) else {
            return HashMap::new();
        };
        rels.root_element()
            .children()
            .filter(|node| node.has_tag_name((PACKAGE_RELATIONSHIPS, "Relationship")))
            .filter_map(|node| {
                let external = node.attribute("TargetMode") == Some("External");
                let target = node.attribute("Target")?;
                Some((
                    node.attribute("Id")?.to_owned(),
                    Relation {
                        kind: node.attribute("Type")?.to_owned(),
                        target: if external {
                            target.to_owned()
                        } else {
                            join_part(folder, target)
                        },
                        external,
                    },
                ))
            })
            .collect()
    }
}

pub const PACKAGE_RELATIONSHIPS: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships";
pub const RELATIONSHIPS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STRICT_RELATIONSHIPS: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";

pub fn relation_id<'a>(node: roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| {
            attribute.name() == name
                && matches!(
                    attribute.namespace(),
                    Some(RELATIONSHIPS | STRICT_RELATIONSHIPS)
                )
        })
        .map(|attribute| attribute.value())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub kind: String,
    pub target: String,
    pub external: bool,
}

fn join_part(folder: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        folder.split('/').filter(|part| !part.is_empty()).collect()
    };
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

/// Maps part paths to replacement bytes, or `None` to remove a part.
pub type Parts = BTreeMap<String, Option<Vec<u8>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

#[must_use]
pub fn spans(line: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut rest = line;
    let mut text = String::new();
    let (mut bold, mut italic) = (false, false);
    while let Some(at) = rest.find('*') {
        let mark = if rest[at..].starts_with("**") {
            "**"
        } else {
            "*"
        };
        let after = &rest[at + mark.len()..];
        let open = if mark == "**" { bold } else { italic };
        if !open && !after.contains(mark) {
            text.push_str(&rest[..at + mark.len()]);
            rest = after;
            continue;
        }
        text.push_str(&rest[..at]);
        rest = after;
        if !text.is_empty() {
            out.push(Span {
                text: std::mem::take(&mut text),
                bold,
                italic,
            });
        }
        if mark == "**" {
            bold.toggle();
        } else {
            italic.toggle();
        }
    }
    text.push_str(rest);
    if !text.is_empty() {
        out.push(Span { text, bold, italic });
    }
    out
}

/// Rewrites the ZIP package, raw-copying unchanged entries without recompressing.
pub fn rewrite(file: &[u8], parts: &Parts) -> Result<Vec<u8>, PackageError> {
    let mut archive = ZipArchive::new(Cursor::new(file)).map_err(broken)?;
    let mut writer = ZipWriter::new(Cursor::new(Vec::with_capacity(file.len())));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut written = HashSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index).map_err(broken)?;
        let name = entry.name().map_err(broken)?.into_owned();
        match parts.get(&name) {
            None => writer.raw_copy_file(entry).map_err(broken)?,
            Some(None) => {}
            Some(Some(bytes)) => {
                drop(entry);
                writer.start_file(&name, options).map_err(broken)?;
                writer.write_all(bytes).map_err(broken)?;
            }
        }
        written.insert(name);
    }
    for (name, bytes) in parts {
        if let Some(bytes) = bytes
            && !written.contains(name)
        {
            writer.start_file(name, options).map_err(broken)?;
            writer.write_all(bytes).map_err(broken)?;
        }
    }
    Ok(writer.finish().map_err(broken)?.into_inner())
}

pub fn create<N: AsRef<str>, B: AsRef<[u8]>>(
    parts: impl IntoIterator<Item = (N, B)>,
) -> Result<Vec<u8>, PackageError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, bytes) in parts {
        writer.start_file(name.as_ref(), options).map_err(broken)?;
        writer.write_all(bytes.as_ref()).map_err(broken)?;
    }
    Ok(writer.finish().map_err(broken)?.into_inner())
}

/// Escapes XML metacharacters and strips characters disallowed by XML 1.0.
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for letter in text.chars() {
        match letter {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\t'
            | '\n'
            | '\r'
            | '\u{20}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}' => escaped.push(letter),
            _ => {}
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        package_after(b"", entries)
    }

    /// Builds a ZIP after `front` bytes with offsets adjusted past the prefix (like `zip -A`).
    fn package_after(front: &[u8], entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut file = Cursor::new(front.to_vec());
        file.set_position(u64::try_from(front.len()).unwrap());
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn crc32(bytes: &[u8]) -> u32 {
        !bytes.iter().fold(!0, |crc, &byte| {
            (0..8).fold(crc ^ u32::from(byte), |crc, _| {
                (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg())
            })
        })
    }

    /// Adds Info-ZIP Unicode Path extra fields (`0x7075`) so parsers that honor them see `second`
    /// while others see `name`.
    fn package_with_second_names(entries: &[(&str, &str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, second, bytes) in entries {
            let field = [
                &[1][..],
                &crc32(name.as_bytes()).to_le_bytes(),
                second.as_bytes(),
            ]
            .concat();
            let mut options = zip::write::FullFileOptions::default();
            options.add_extra_field(0x7075, field, false).unwrap();
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn an_entry_that_leaves_the_package_is_refused() {
        let file = package_of(&[("../evil", b"x")]);
        assert!(matches!(Package::open(&file), Err(PackageError::Unsafe(_))));
    }

    #[test]
    fn a_package_that_another_program_reads_otherwise_is_refused() {
        let file = package_of(&[("a.xml", b"<a/>"), ("b.xml", b"<b/>")]);
        let mut twice = file.clone();
        for at in 0..twice.len() - 5 {
            if &twice[at..at + 5] == b"b.xml" {
                twice[at] = b'a';
            }
        }
        let mut renamed = file.clone();
        let central = renamed
            .windows(4)
            .rposition(|bytes| bytes == b"PK\x01\x02")
            .unwrap();
        renamed[central + 46..central + 51].copy_from_slice(b"c.xml");
        let mut swapped = package_with_second_names(&[
            ("word/document.xml", "word/documentä.xml", b"<a/>"),
            ("word/documentä.xml", "word/document.xml", b"<b/>"),
        ]);
        // The `zip` crate refuses a differing Unicode Path extra when the UTF-8 flag (bit 11) is
        // set, so an attack leaves that flag clear; clear it here to exercise the spoofing check.
        for (header, high_flags) in [(&b"PK\x03\x04"[..], 7), (b"PK\x01\x02", 9)] {
            let at = swapped
                .windows(4)
                .rposition(|bytes| bytes == header)
                .unwrap();
            swapped[at + high_flags] &= !0x08;
        }
        let cased = package_of(&[
            ("word/document.xml", b"<a/>"),
            ("word/Document.xml", b"<b/>"),
        ]);
        let behind = [b"MZ".as_slice(), &file].concat();
        let counted = package_after(b"MZ", &[("a.xml", b"<a/>")]);
        let mut hidden = package_of(&[("a.xml", b"<a/>"), ("c.xml", b"<c/>"), ("b.xml", b"<b/>")]);
        let end = hidden.len() - 22;
        hidden[end + 8..end + 12].copy_from_slice(&[2, 0, 2, 0]);
        for (case, file) in [
            ("two entries of one name", twice),
            ("a local header with another name", renamed),
            ("second names that swap two entries", swapped),
            ("names that differ in case", cased),
            ("data in front", behind),
            ("data in front that the offsets count", counted),
            ("an entry that the end record does not count", hidden),
        ] {
            assert!(
                matches!(Package::open(&file), Err(PackageError::Unsafe(_))),
                "{case}"
            );
        }
    }

    #[test]
    fn a_local_header_with_its_own_time_opens() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let mut options = zip::write::FullFileOptions::default();
        let access_time_flag_with_three_times = [&[2][..], &[0; 12]].concat();
        options
            .add_extra_field(EXTENDED_TIMESTAMP, access_time_flag_with_three_times, false)
            .unwrap();
        writer.start_file("a.xml", options).unwrap();
        writer.write_all(b"<a/>").unwrap();
        let mut file = writer.finish().unwrap().into_inner();
        file[TIME_IN_LOCAL_HEADER..TIME_IN_LOCAL_HEADER + TIME_BYTES]
            .copy_from_slice(&[0, 0, 0x22, 0]);
        let mut package = Package::open(&file).unwrap();
        assert_eq!(package.bytes("a.xml").unwrap(), b"<a/>");
    }

    #[test]
    fn a_zip_bomb_is_refused() {
        let file = package_of(&[("big", &vec![0; 64 * 1024 * 1024])]);
        assert!(matches!(Package::open(&file), Err(PackageError::Unsafe(_))));
    }
}
