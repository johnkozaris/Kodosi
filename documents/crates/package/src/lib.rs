//! An Office file as a package of parts. A document can come from anyone, so reading limits the
//! entries, the unpacked size and the compression ratio, and it checks each entry name. Writing
//! copies each part that did not change byte for byte.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::{Cursor, Read as _, Write as _},
};

use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MOST_ENTRIES: usize = 16 * 1024;
const MOST_UNPACKED: u64 = 512 * 1024 * 1024;
const MOST_RATIO: u64 = 1000;
const RATIO_FROM: u64 = 1024 * 1024;

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

/// A checked package.
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
        let mut unpacked = 0_u64;
        let mut names = HashSet::new();
        for index in 0..archive.len() {
            let entry = archive.by_index_raw(index).map_err(broken)?;
            let name = entry.name().map_err(broken)?.into_owned();
            if !safe_name(&name) {
                return Err(PackageError::Unsafe(format!("the entry name {name}")));
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
            names.insert(name);
        }
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

    /// The relationships of a part: each id with its target, as a part name when it is inside
    /// the package.
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
            .filter(|node| node.has_tag_name((RELATIONSHIPS, "Relationship")))
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

const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub kind: String,
    pub target: String,
    pub external: bool,
}

/// The part that a relative target names, seen from a folder.
pub fn join_part(folder: &str, target: &str) -> String {
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

/// New bytes for parts of a package. `None` removes a part.
pub type Parts = BTreeMap<String, Option<Vec<u8>>>;

/// Writes a package with some parts changed. All others keep their compressed bytes.
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

/// Text for XML content.
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for letter in text.chars() {
        match letter {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            letter => escaped.push(letter),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in entries {
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
    fn a_zip_bomb_is_refused() {
        let file = package_of(&[("big", &vec![0; 64 * 1024 * 1024])]);
        assert!(matches!(Package::open(&file), Err(PackageError::Unsafe(_))));
    }

    #[test]
    fn a_rewrite_changes_one_part_and_keeps_the_others_exact() {
        let file = package_of(&[("a.xml", b"<a/>"), ("b.xml", b"<b/>")]);
        let parts = Parts::from([("a.xml".to_owned(), Some(b"<new/>".to_vec()))]);
        let rewritten = rewrite(&file, &parts).unwrap();
        let mut package = Package::open(&rewritten).unwrap();
        assert_eq!(package.bytes("a.xml").unwrap(), b"<new/>");
        assert_eq!(package.bytes("b.xml").unwrap(), b"<b/>");
    }

    #[test]
    fn a_rewrite_adds_and_removes_parts() {
        let file = package_of(&[("a.xml", b"<a/>"), ("b.xml", b"<b/>")]);
        let parts = Parts::from([
            ("a.xml".to_owned(), None),
            ("c.xml".to_owned(), Some(b"<c/>".to_vec())),
        ]);
        let rewritten = rewrite(&file, &parts).unwrap();
        let mut package = Package::open(&rewritten).unwrap();
        assert!(!package.has("a.xml"));
        assert_eq!(package.bytes("c.xml").unwrap(), b"<c/>");
    }

    #[test]
    fn relations_become_part_names() {
        let rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
            <Relationship Id="rId1" Type="image" Target="media/one.png"/>
            <Relationship Id="rId2" Type="image" Target="../up.png"/>
            <Relationship Id="rId3" Type="link" Target="https://a.b/" TargetMode="External"/>
        </Relationships>"#;
        let file = package_of(&[("word/_rels/document.xml.rels", rels)]);
        let mut package = Package::open(&file).unwrap();
        let relations = package.relations("word/document.xml");
        assert_eq!(relations["rId1"].target, "word/media/one.png");
        assert_eq!(relations["rId2"].target, "up.png");
        assert_eq!(relations["rId3"].target, "https://a.b/");
        assert!(relations["rId3"].external);
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(escape(r#"a<b & "c""#), "a&lt;b &amp; &quot;c&quot;");
    }
}
