use std::sync::Arc;

use document::{Assets, Guide, Ordered, Refused, guide_field as field, guide_op as op};
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    model::{Change, Model, Note, PageWork, Query},
    read::{hex_of, pages_of},
    redact, write,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    AddNote {
        page: u32,
        text: String,
        x: f64,
        y: f64,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
        #[serde(default)]
        kind: Option<String>,
    },
    RemoveNote {
        note: String,
    },
    SetField {
        name: String,
        #[serde(default)]
        value: Option<String>,
    },
    MovePage {
        page: u32,
        to: u32,
    },
    RotatePage {
        page: u32,
        degrees: i64,
    },
    RemovePage {
        page: u32,
    },
    Redact {
        page: u32,
        rects: Vec<[f64; 4]>,
    },
}

pub(crate) fn guide() -> Guide {
    let page = || field("page", "page number", true, "From 1");
    Guide {
        format: "pdf".to_owned(),
        ops: vec![
            op(
                "addNote",
                "Adds a note to a page",
                vec![
                    page(),
                    field("text", "text", true, "Its text"),
                    field("x", "points", true, "From the left of the page"),
                    field("y", "points", true, "From the bottom of the page"),
                    field("width", "points", false, "Its width"),
                    field("height", "points", false, "Its height"),
                    field("kind", "note kind", false, "Text, FreeText or Highlight"),
                ],
            ),
            op(
                "removeNote",
                "Removes a note",
                vec![field("note", "note id", true, "The note")],
            ),
            op(
                "setField",
                "Sets a form value",
                vec![
                    field("name", "field name", true, "The full name of the field"),
                    field(
                        "value",
                        "text",
                        false,
                        "Its value. Without it, the field is empty",
                    ),
                ],
            ),
            op(
                "movePage",
                "Moves a page",
                vec![page(), field("to", "page number", true, "Its new place")],
            ),
            op(
                "rotatePage",
                "Turns a page",
                vec![page(), field("degrees", "number", true, "90, 180 or 270")],
            ),
            op("removePage", "Removes a page", vec![page()]),
            op(
                "redact",
                "Removes what a page shows in these places, for good",
                vec![
                    page(),
                    field(
                        "rects",
                        "rectangles",
                        true,
                        "Each one as left, bottom, right, top, in points",
                    ),
                ],
            ),
        ],
    }
}

#[expect(clippy::needless_pass_by_value, reason = "it maps errors")]
fn pdf_failed(error: lopdf::Error) -> Refused {
    Refused::Invalid(error.to_string())
}

fn page_object(model: &Model, id: &str) -> Result<lopdf::ObjectId, Refused> {
    model
        .pages
        .iter()
        .find(|page| page.id == id)
        .and_then(write::id_of)
        .ok_or_else(|| Refused::Invalid(format!("There is no page {id}.")))
}

fn do_work(model: &Model, work: &PageWork) -> Result<Vec<u8>, Refused> {
    match work {
        PageWork::Order { pages } => {
            let wanted: Vec<lopdf::ObjectId> = pages
                .iter()
                .map(|page| page_object(model, page))
                .collect::<Result<_, _>>()?;
            write::pages_update(&model.file, |_, order| {
                *order = wanted.into_iter().map(Object::Reference).collect();
                Ok(())
            })
            .map_err(pdf_failed)
        }
        PageWork::Rotate { page, degrees } => {
            if degrees % 90 != 0 {
                return Err(Refused::Invalid(
                    "A page turns by 90, 180 or 270 degrees.".to_owned(),
                ));
            }
            let id = page_object(model, page)?;
            let current = model
                .pages
                .iter()
                .find(|known| known.id == *page)
                .map_or(0, |known| known.rotation);
            let rotation = (current + degrees).rem_euclid(360);
            write::pages_update(&model.file, |document, _| {
                document
                    .get_dictionary_mut(id)?
                    .set("Rotate", Object::Integer(rotation));
                Ok(())
            })
            .map_err(pdf_failed)
        }
        PageWork::Remove { page } => {
            if model.pages.len() == 1 {
                return Err(Refused::Invalid("A PDF keeps its last page.".to_owned()));
            }
            let id = page_object(model, page)?;
            write::pages_update(&model.file, |_, order| {
                order.retain(|known| known.as_reference().ok() != Some(id));
                Ok(())
            })
            .map_err(pdf_failed)
        }
        PageWork::Redact { page, rects } => {
            redact::redact(&model.file, page_object(model, page)?, rects).map_err(Refused::Invalid)
        }
    }
}

fn take_file(model: &mut Model, bytes: Vec<u8>, assets: &mut Assets) -> Result<(), Refused> {
    let document = Document::load_mem(&bytes).map_err(pdf_failed)?;
    model.pages = pages_of(&document).map_err(|error| Refused::Invalid(error.0))?;
    model.asset = hex_of(&assets.put(&bytes));
    model.file = Arc::new(bytes);
    Ok(())
}

fn work_on(model: &mut Model, work: PageWork, assets: &mut Assets) -> Result<Change, Refused> {
    let bytes = do_work(model, &work)?;
    take_file(model, bytes, assets)?;
    if matches!(work, PageWork::Redact { .. } | PageWork::Remove { .. }) {
        let pages: Vec<String> = model.pages.iter().map(|page| page.id.clone()).collect();
        model.notes.retain(|_, note| pages.contains(&note.page));
        let document = Document::load_mem(&model.file).map_err(pdf_failed)?;
        let mut scratch = Assets::default();
        let fresh = crate::read::read(&model.file, &mut scratch)
            .map_err(|error| Refused::Invalid(error.0))?;
        model.file_notes = fresh.file_notes;
        model.file_fields = crate::read::form_fields(&document);
    }
    Ok(Change::Worked {
        work,
        asset: model.asset.clone(),
        pages: model.pages.clone(),
    })
}

fn apply_one(model: &mut Model, change: Change, assets: &mut Assets) -> Result<Change, Refused> {
    Ok(match change {
        Change::Note { id, note } => {
            match &note {
                Some(note) => {
                    model.notes.insert(id.clone(), note.clone());
                }
                None => {
                    model.notes.remove(&id);
                }
            }
            Change::Note { id, note }
        }
        Change::Field { name, value } => {
            match &value {
                Some(value) => {
                    model.fields.insert(name.clone(), value.clone());
                }
                None => {
                    model.fields.remove(&name);
                }
            }
            Change::Field { name, value }
        }
        Change::Work { work } | Change::Worked { work, .. } => work_on(model, work, assets)?,
        Change::Base { asset } => {
            let hash = hash_from_hex(&asset)
                .ok_or_else(|| Refused::Invalid("The asset has no valid hash.".to_owned()))?;
            let bytes = assets
                .get(&hash)
                .ok_or_else(|| Refused::Invalid(format!("There is no asset {asset}.")))?
                .to_vec();
            take_file(model, bytes, assets)?;
            Change::Base { asset }
        }
        Change::Group { changes } => Change::Group {
            changes: changes
                .into_iter()
                .map(|change| apply_one(model, change, assets))
                .collect::<Result<_, _>>()?,
        },
        Change::Ops { .. } => return Err(Refused::Invalid("Ops cannot hold ops.".to_owned())),
    })
}

fn hash_from_hex(text: &str) -> Option<[u8; 32]> {
    let mut hash = [0; 32];
    if text.len() != 64 || !text.is_ascii() {
        return None;
    }
    for (index, pair) in text.as_bytes().chunks(2).enumerate() {
        hash[index] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(hash)
}

fn page_at(model: &Model, number: u32) -> Result<String, Refused> {
    let index = usize::try_from(number.saturating_sub(1)).unwrap_or(usize::MAX);
    model
        .pages
        .get(index)
        .map(|page| page.id.clone())
        .ok_or_else(|| Refused::Invalid(format!("There is no page {number}.")))
}

fn new_note_id(model: &Model, taken: usize) -> String {
    let highest = model
        .notes
        .keys()
        .filter_map(|id| id.strip_prefix("note-"))
        .filter_map(|number| number.parse::<usize>().ok())
        .max()
        .unwrap_or(0);
    format!("note-{}", highest + taken + 1)
}

fn to_changes(model: &Model, ops: Vec<Op>) -> Result<Vec<Change>, Refused> {
    let mut changes = Vec::with_capacity(ops.len());
    for (taken, op) in ops.into_iter().enumerate() {
        changes.push(match op {
            Op::AddNote {
                page,
                text,
                x,
                y,
                width,
                height,
                kind,
            } => Change::Note {
                id: new_note_id(model, taken),
                note: Some(Note {
                    page: page_at(model, page)?,
                    kind: kind.unwrap_or_else(|| "Text".to_owned()),
                    rect: [x, y, x + width.unwrap_or(24.0), y + height.unwrap_or(24.0)],
                    text,
                    color: Some("FFD400".to_owned()),
                    author: String::new(),
                }),
            },
            Op::RemoveNote { note } => {
                if !model.notes.contains_key(&note) {
                    return Err(Refused::Invalid(format!("There is no note {note}.")));
                }
                Change::Note {
                    id: note,
                    note: None,
                }
            }
            Op::SetField { name, value } => {
                let known =
                    model.fields.contains_key(&name) || model.file_fields.contains_key(&name);
                if !known && value.is_none() {
                    return Err(Refused::Invalid(format!("There is no field {name}.")));
                }
                Change::Field { name, value }
            }
            Op::MovePage { page, to } => {
                let mut pages: Vec<String> =
                    model.pages.iter().map(|page| page.id.clone()).collect();
                let moved = page_at(model, page)?;
                pages.retain(|known| *known != moved);
                let index = usize::try_from(to.saturating_sub(1))
                    .unwrap_or(0)
                    .min(pages.len());
                pages.insert(index, moved);
                Change::Work {
                    work: PageWork::Order { pages },
                }
            }
            Op::RotatePage { page, degrees } => Change::Work {
                work: PageWork::Rotate {
                    page: page_at(model, page)?,
                    degrees,
                },
            },
            Op::RemovePage { page } => Change::Work {
                work: PageWork::Remove {
                    page: page_at(model, page)?,
                },
            },
            Op::Redact { page, rects } => Change::Work {
                work: PageWork::Redact {
                    page: page_at(model, page)?,
                    rects,
                },
            },
        });
    }
    Ok(changes)
}

fn check_conflicts(ops: &[Op], since: &[Ordered<Change>]) -> Result<(), Refused> {
    for ordered in since {
        let (notes, fields) = keys_of(&ordered.change);
        for op in ops {
            let met = match op {
                Op::RemoveNote { note } => notes.contains(note),
                Op::SetField { name, .. } => fields.contains(name),
                _ => false,
            };
            if met {
                return Err(Refused::Behind(format!(
                    "{} changed this after you read it. Read again.",
                    ordered.author
                )));
            }
        }
    }
    Ok(())
}

fn keys_of(change: &Change) -> (Vec<String>, Vec<String>) {
    match change {
        Change::Note { id, .. } => (vec![id.clone()], Vec::new()),
        Change::Field { name, .. } => (Vec::new(), vec![name.clone()]),
        Change::Group { changes } => {
            let mut notes = Vec::new();
            let mut fields = Vec::new();
            for change in changes {
                let (more_notes, more_fields) = keys_of(change);
                notes.extend(more_notes);
                fields.extend(more_fields);
            }
            (notes, fields)
        }
        _ => (Vec::new(), Vec::new()),
    }
}

pub(crate) fn apply(
    model: &mut Model,
    change: Change,
    since: &[Ordered<Change>],
    assets: &mut Assets,
) -> Result<Change, Refused> {
    let change = if let Change::Ops { ops } = change {
        check_conflicts(&ops, since)?;
        Change::Group {
            changes: to_changes(model, ops)?,
        }
    } else {
        change
    };
    let mut copy = model.clone();
    let done = apply_one(&mut copy, change, assets)?;
    *model = copy;
    Ok(done)
}

pub(crate) fn compare(from: &Model, to: &Model) -> Vec<Change> {
    let mut changes = Vec::new();
    if from.pages != to.pages {
        changes.push(Change::Base {
            asset: to.asset.clone(),
        });
    }
    for (id, note) in &to.notes {
        if from.notes.get(id) != Some(note) {
            changes.push(Change::Note {
                id: id.clone(),
                note: Some(note.clone()),
            });
        }
    }
    for id in from.notes.keys() {
        if !to.notes.contains_key(id) {
            changes.push(Change::Note {
                id: id.clone(),
                note: None,
            });
        }
    }
    for (name, value) in &to.fields {
        if from.fields.get(name) != Some(value) {
            changes.push(Change::Field {
                name: name.clone(),
                value: Some(value.clone()),
            });
        }
    }
    changes
}

pub(crate) fn answer(model: &Model, query: &Query) -> Value {
    match query {
        Query::Read { page } => {
            let document = Document::load_mem(&model.file).ok();
            let numbers: Vec<u32> = page.map_or_else(
                || (1..=u32::try_from(model.pages.len()).unwrap_or(0)).collect(),
                |page| vec![page],
            );
            let pages: Vec<Value> = numbers
                .iter()
                .map(|number| {
                    let text = document
                        .as_ref()
                        .and_then(|document| document.extract_text(&[*number]).ok())
                        .unwrap_or_default();
                    json!({ "page": number, "text": text })
                })
                .collect();
            json!({ "pages": pages, "notes": model.notes, "fields": model.fields })
        }
        Query::Check => {
            let notes = model.notes.len();
            json!({ "pages": model.pages.len(), "openNotes": notes })
        }
    }
}
