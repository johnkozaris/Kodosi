use std::{collections::BTreeMap, fmt::Write as _, sync::Arc};

use document::{Assets, Unreadable};
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::model::{Model, Note, Page};

const MOST_FIELD_DEPTH: usize = 32;
const NOTE_KINDS: &[&str] = &[
    "Text",
    "FreeText",
    "Highlight",
    "Underline",
    "StrikeOut",
    "Square",
    "Circle",
    "Line",
    "Ink",
];

fn broken(error: impl std::fmt::Display) -> Unreadable {
    Unreadable(error.to_string())
}

pub(crate) fn page_id(id: ObjectId) -> String {
    format!("{}-{}", id.0, id.1)
}

pub(crate) fn number_of(object: &Object) -> Option<f64> {
    match object {
        #[expect(clippy::cast_precision_loss, reason = "page numbers are far smaller")]
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

fn numbers(document: &Document, object: &Object) -> Option<Vec<f64>> {
    let (_, object) = document.dereference(object).ok()?;
    object.as_array().ok()?.iter().map(number_of).collect()
}

pub(crate) fn inherited<'d>(
    document: &'d Document,
    page: &'d Dictionary,
    key: &[u8],
) -> Option<&'d Object> {
    let mut current = page;
    for _ in 0..MOST_FIELD_DEPTH {
        if let Ok(value) = current.get(key) {
            return Some(value);
        }
        let parent = current.get(b"Parent").ok()?.as_reference().ok()?;
        current = document.get_dictionary(parent).ok()?;
    }
    None
}

pub(crate) fn text_of(bytes: &[u8]) -> String {
    if let Some(wide) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let units: Vec<u16> = wide
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

fn string_of(document: &Document, dictionary: &Dictionary, key: &[u8]) -> Option<String> {
    let (_, object) = document.dereference(dictionary.get(key).ok()?).ok()?;
    match object {
        Object::String(bytes, _) => Some(text_of(bytes)),
        Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
        _ => None,
    }
}

fn color_of(document: &Document, dictionary: &Dictionary) -> Option<String> {
    let parts = numbers(document, dictionary.get(b"C").ok()?)?;
    let [red, green, blue] = parts.as_slice() else {
        return None;
    };
    let mut hex = String::with_capacity(6);
    for part in [red, green, blue] {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped"
        )]
        let byte = (part.clamp(0.0, 1.0) * 255.0).round() as u8;
        let _ = write!(hex, "{byte:02X}");
    }
    Some(hex)
}

fn page_notes(
    document: &Document,
    page: ObjectId,
    notes: &mut BTreeMap<String, Note>,
) -> Result<(), lopdf::Error> {
    let dictionary = document.get_dictionary(page)?;
    let Ok(annotations) = dictionary.get(b"Annots") else {
        return Ok(());
    };
    let (_, annotations) = document.dereference(annotations)?;
    for (index, annotation) in annotations.as_array()?.iter().enumerate() {
        let id = annotation.as_reference().ok();
        let (_, object) = document.dereference(annotation)?;
        let Ok(found) = object.as_dict() else {
            continue;
        };
        let kind = found
            .get(b"Subtype")
            .and_then(Object::as_name)
            .unwrap_or_default();
        let kind = String::from_utf8_lossy(kind).into_owned();
        if !NOTE_KINDS.contains(&kind.as_str()) {
            continue;
        }
        let Some(rect) = found
            .get(b"Rect")
            .ok()
            .and_then(|rect| numbers(document, rect))
            .and_then(|rect| <[f64; 4]>::try_from(rect).ok())
        else {
            continue;
        };
        let name = id.map_or_else(
            || format!("direct-{}-{index}", page.0),
            |id| format!("a{}-{}", id.0, id.1),
        );
        notes.insert(
            name,
            Note {
                page: page_id(page),
                kind,
                rect,
                text: string_of(document, found, b"Contents").unwrap_or_default(),
                color: color_of(document, found),
                author: string_of(document, found, b"T").unwrap_or_default(),
            },
        );
    }
    Ok(())
}

fn walk_fields(
    document: &Document,
    object: &Object,
    prefix: &str,
    depth: usize,
    fields: &mut BTreeMap<String, String>,
) -> Option<()> {
    if depth > MOST_FIELD_DEPTH {
        return None;
    }
    let (_, resolved) = document.dereference(object).ok()?;
    let field = resolved.as_dict().ok()?;
    let own = string_of(document, field, b"T");
    let name = match (&own, prefix.is_empty()) {
        (Some(own), true) => own.clone(),
        (Some(own), false) => format!("{prefix}.{own}"),
        (None, _) => prefix.to_owned(),
    };
    let kids = field
        .get(b"Kids")
        .ok()
        .and_then(|kids| document.dereference(kids).ok())
        .and_then(|(_, kids)| kids.as_array().ok());
    if let Some(kids) = kids {
        for kid in kids {
            walk_fields(document, kid, &name, depth + 1, fields);
        }
    }
    if !name.is_empty()
        && let Some(value) = string_of(document, field, b"V")
    {
        fields.insert(name, value);
    }
    Some(())
}

pub(crate) fn form_fields(document: &Document) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let list = document
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|form| document.dereference(form).ok())
        .and_then(|(_, form)| form.as_dict().ok())
        .and_then(|form| form.get(b"Fields").ok())
        .and_then(|list| document.dereference(list).ok())
        .and_then(|(_, list)| list.as_array().ok());
    for field in list.into_iter().flatten() {
        walk_fields(document, field, "", 0, &mut fields);
    }
    fields
}

pub(crate) fn pages_of(document: &Document) -> Result<Vec<Page>, Unreadable> {
    let mut pages = Vec::new();
    for id in document.get_pages().into_values() {
        let dictionary = document.get_dictionary(id).map_err(broken)?;
        let bounds = inherited(document, dictionary, b"MediaBox")
            .and_then(|object| numbers(document, object))
            .ok_or_else(|| Unreadable("a page has no size".to_owned()))?;
        let [left, bottom, right, top] = bounds.as_slice() else {
            return Err(Unreadable("a page has a broken size".to_owned()));
        };
        let rotation = inherited(document, dictionary, b"Rotate")
            .and_then(|object| object.as_i64().ok())
            .unwrap_or(0);
        pages.push(Page {
            id: page_id(id),
            width: right - left,
            height: top - bottom,
            rotation,
        });
    }
    Ok(pages)
}

pub(crate) fn hex_of(hash: &[u8]) -> String {
    hash.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

pub(crate) fn read(file: &[u8], assets: &mut Assets) -> Result<Model, Unreadable> {
    let document = Document::load_mem(file).map_err(broken)?;
    if document.is_encrypted() {
        return Err(Unreadable("the file has a password".to_owned()));
    }
    let pages = pages_of(&document)?;
    let mut notes = BTreeMap::new();
    for page in document.get_pages().into_values() {
        page_notes(&document, page, &mut notes).map_err(broken)?;
    }
    let fields = form_fields(&document);
    let asset = hex_of(&assets.put(file));
    Ok(Model {
        file: Arc::new(file.to_vec()),
        asset,
        pages,
        file_notes: notes.clone(),
        notes,
        file_fields: fields.clone(),
        fields,
    })
}
