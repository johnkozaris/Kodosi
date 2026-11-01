use document::Unwritable;
use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, StringFormat};

use crate::{
    model::{Model, Note, Page},
    read::page_id,
};

const MOST_FIELD_DEPTH: usize = 32;

fn failed(error: impl std::fmt::Display) -> Unwritable {
    Unwritable(error.to_string())
}

fn object_id(id: &str) -> Option<ObjectId> {
    let (number, generation) = id.trim_start_matches('a').split_once('-')?;
    Some((number.parse().ok()?, generation.parse().ok()?))
}

pub(crate) fn pdf_text(text: &str) -> Object {
    if text.is_ascii() {
        return Object::String(text.as_bytes().to_vec(), StringFormat::Literal);
    }
    let mut bytes = vec![0xfe, 0xff];
    bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
    Object::String(bytes, StringFormat::Hexadecimal)
}

fn color_parts(color: &str) -> Option<Vec<Object>> {
    let value = u32::from_str_radix(color, 16).ok()?;
    let part = |shift: u32| {
        let byte = u8::try_from((value >> shift) & 0xff).unwrap_or(0);
        Object::Real(f32::from(byte) / 255.0)
    };
    Some(vec![part(16), part(8), part(0)])
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "PDF numbers are this exact"
)]
fn real(value: f64) -> Object {
    Object::Real(value as f32)
}

fn note_dictionary(note: &Note, page: ObjectId) -> Dictionary {
    let [left, bottom, right, top] = note.rect;
    let mut annotation = Dictionary::new();
    annotation.set("Type", Object::Name(b"Annot".to_vec()));
    annotation.set("Subtype", Object::Name(note.kind.clone().into_bytes()));
    annotation.set(
        "Rect",
        Object::Array(vec![real(left), real(bottom), real(right), real(top)]),
    );
    annotation.set("Contents", pdf_text(&note.text));
    annotation.set("T", pdf_text(&note.author));
    annotation.set("P", Object::Reference(page));
    annotation.set("F", Object::Integer(4));
    if let Some(parts) = note.color.as_deref().and_then(color_parts) {
        annotation.set("C", Object::Array(parts));
    }
    match note.kind.as_str() {
        "Highlight" | "Underline" | "StrikeOut" => {
            annotation.set(
                "QuadPoints",
                Object::Array(vec![
                    real(left),
                    real(top),
                    real(right),
                    real(top),
                    real(left),
                    real(bottom),
                    real(right),
                    real(bottom),
                ]),
            );
        }
        "FreeText" => {
            annotation.set(
                "DA",
                Object::String(b"0 0 0 rg /Helv 12 Tf".to_vec(), StringFormat::Literal),
            );
        }
        _ => {}
    }
    annotation
}

/// The array of a page's notes, in the new part of the file.
fn page_annotations(
    update: &mut IncrementalDocument,
    page: ObjectId,
) -> Result<&mut Vec<Object>, lopdf::Error> {
    update.opt_clone_object_to_new_document(page)?;
    let reference = {
        let dictionary = update.new_document.get_dictionary(page)?;
        dictionary
            .get(b"Annots")
            .ok()
            .and_then(|annotations| annotations.as_reference().ok())
    };
    if let Some(reference) = reference {
        update.opt_clone_object_to_new_document(reference)?;
        return update
            .new_document
            .get_object_mut(reference)?
            .as_array_mut();
    }
    let dictionary = update.new_document.get_dictionary_mut(page)?;
    if !dictionary.has(b"Annots") {
        dictionary.set("Annots", Object::Array(Vec::new()));
    }
    dictionary.get_mut(b"Annots")?.as_array_mut()
}

fn save_notes(model: &Model, update: &mut IncrementalDocument) -> Result<(), String> {
    let error = |error: lopdf::Error| error.to_string();
    for (id, note) in &model.notes {
        if model.file_notes.get(id) == Some(note) {
            continue;
        }
        let page = object_id(&note.page).ok_or("a note has no page")?;
        if let (Some(_), Some(object)) = (model.file_notes.get(id), object_id(id)) {
            update
                .opt_clone_object_to_new_document(object)
                .map_err(error)?;
            let dictionary = update
                .new_document
                .get_dictionary_mut(object)
                .map_err(error)?;
            for (key, value) in note_dictionary(note, page) {
                dictionary.set(key, value);
            }
            continue;
        }
        let dictionary = note_dictionary(note, page);
        let added = update.new_document.add_object(dictionary);
        page_annotations(update, page)
            .map_err(error)?
            .push(Object::Reference(added));
    }
    for (id, note) in &model.file_notes {
        if model.notes.contains_key(id) {
            continue;
        }
        let (Some(page), Some(object)) = (object_id(&note.page), object_id(id)) else {
            continue;
        };
        page_annotations(update, page)
            .map_err(error)?
            .retain(|annotation| annotation.as_reference().ok() != Some(object));
    }
    Ok(())
}

fn find_field(
    document: &Document,
    object: &Object,
    prefix: &str,
    wanted: &str,
    depth: usize,
) -> Option<ObjectId> {
    if depth > MOST_FIELD_DEPTH {
        return None;
    }
    let reference = object.as_reference().ok();
    let (_, resolved) = document.dereference(object).ok()?;
    let field = resolved.as_dict().ok()?;
    let own = field
        .get(b"T")
        .ok()
        .and_then(|name| name.as_str().ok())
        .map(crate::read::text_of);
    let name = match (&own, prefix.is_empty()) {
        (Some(own), true) => own.clone(),
        (Some(own), false) => format!("{prefix}.{own}"),
        (None, _) => prefix.to_owned(),
    };
    if name == wanted {
        return reference;
    }
    let (_, kids) = document.dereference(field.get(b"Kids").ok()?).ok()?;
    kids.as_array()
        .ok()?
        .iter()
        .find_map(|kid| find_field(document, kid, &name, wanted, depth + 1))
}

fn save_fields(model: &Model, update: &mut IncrementalDocument) -> Result<(), String> {
    let error = |error: lopdf::Error| error.to_string();
    let changed: Vec<(&String, Option<&String>)> = model
        .fields
        .iter()
        .filter(|(name, value)| model.file_fields.get(*name) != Some(value))
        .map(|(name, value)| (name, Some(value)))
        .chain(
            model
                .file_fields
                .keys()
                .filter(|name| !model.fields.contains_key(*name))
                .map(|name| (name, None)),
        )
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    let previous = update.get_prev_documents().clone();
    let form_object = previous
        .catalog()
        .map_err(error)?
        .get(b"AcroForm")
        .map_err(error)?
        .clone();
    let (form_id, form) = previous.dereference(&form_object).map_err(error)?;
    let fields = form
        .as_dict()
        .map_err(error)?
        .get(b"Fields")
        .map_err(error)?
        .clone();
    let (_, list) = previous.dereference(&fields).map_err(error)?;
    for (name, value) in changed {
        let found = list
            .as_array()
            .map_err(error)?
            .iter()
            .find_map(|field| find_field(&previous, field, "", name, 0))
            .ok_or_else(|| format!("no field {name}"))?;
        update
            .opt_clone_object_to_new_document(found)
            .map_err(error)?;
        let dictionary = update
            .new_document
            .get_dictionary_mut(found)
            .map_err(error)?;
        match value {
            Some(value) => dictionary.set("V", pdf_text(value)),
            None => {
                dictionary.remove(b"V");
            }
        }
    }
    if let Some(form_id) = form_id {
        update
            .opt_clone_object_to_new_document(form_id)
            .map_err(error)?;
        update
            .new_document
            .get_dictionary_mut(form_id)
            .map_err(error)?
            .set("NeedAppearances", Object::Boolean(true));
    }
    Ok(())
}

/// Appends notes and form values to the file, so each byte before them stays.
pub(crate) fn write(model: &Model, file: &[u8]) -> Result<Vec<u8>, Unwritable> {
    let changed = model.notes != model.file_notes || model.fields != model.file_fields;
    if !changed {
        return if model.file.as_slice() == file {
            Ok(file.to_vec())
        } else {
            Ok(model.file.to_vec())
        };
    }
    let document = Document::load_mem(&model.file).map_err(failed)?;
    let mut update = IncrementalDocument::create_from(model.file.to_vec(), document);
    save_notes(model, &mut update).map_err(failed)?;
    save_fields(model, &mut update).map_err(failed)?;
    let mut bytes = Vec::with_capacity(model.file.len() + 4096);
    update.save_to(&mut bytes).map_err(failed)?;
    Ok(bytes)
}

fn set_page_list(
    document: &mut Document,
    root: ObjectId,
    order: &[Object],
) -> Result<(), lopdf::Error> {
    for page in order {
        let id = page.as_reference()?;
        document
            .get_dictionary_mut(id)?
            .set("Parent", Object::Reference(root));
    }
    let pages = document.get_dictionary_mut(root)?;
    pages.set("Kids", Object::Array(order.to_vec()));
    pages.set(
        "Count",
        Object::Integer(i64::try_from(order.len()).unwrap_or(0)),
    );
    Ok(())
}

/// Does page work as an incremental update.
pub(crate) fn pages_update(
    file: &[u8],
    change: impl FnOnce(&mut Document, &mut Vec<Object>) -> Result<(), lopdf::Error>,
) -> Result<Vec<u8>, lopdf::Error> {
    let document = Document::load_mem(file)?;
    let mut order: Vec<Object> = document
        .get_pages()
        .into_values()
        .map(Object::Reference)
        .collect();
    let mut update = IncrementalDocument::create_from(file.to_vec(), document.clone());
    let root = document.catalog()?.get(b"Pages")?.as_reference()?;
    update.opt_clone_object_to_new_document(root)?;
    for page in &order {
        update.opt_clone_object_to_new_document(page.as_reference()?)?;
    }
    change(&mut update.new_document, &mut order)?;
    set_page_list(&mut update.new_document, root, &order)?;
    let mut bytes = Vec::with_capacity(file.len() + 1024);
    update.save_to(&mut bytes)?;
    Ok(bytes)
}

/// A page's name in the file, for tests and ops.
pub(crate) fn id_of(page: &Page) -> Option<ObjectId> {
    object_id(&page.id).filter(|id| page_id(*id) == page.id)
}
