//! PDF format support. Annotations and form values are saved via incremental updates to preserve
//! existing bytes, while page operations run on the host; redactions perform a full rewrite so
//! removed content cannot survive in prior revisions.

mod model;
mod ops;
mod read;
mod redact;
mod write;

use document::{Answer, Assets, Format, Guide, Ordered, Refused, Snapshot, Unreadable, Unwritable};
use serde_json::{Value, json};

pub use model::{Change, Model, Note, Page, PageWork, Pdf, Query};
pub use ops::Op;

impl Format for Pdf {
    type Model = Model;
    type Change = Change;
    type Query = Query;

    fn read(file: &[u8], assets: &mut Assets) -> Result<Model, Unreadable> {
        read::read(file, assets)
    }

    fn snapshot(model: &Model) -> Snapshot {
        Snapshot(model.snapshot())
    }

    fn apply(
        model: &mut Model,
        change: Change,
        since: &[Ordered<Change>],
        assets: &mut Assets,
    ) -> Result<Change, Refused> {
        ops::apply(model, change, since, assets)
    }

    fn compare(from: &Model, to: &Model) -> Vec<Change> {
        ops::compare(from, to)
    }

    fn write(model: &Model, file: &[u8], _assets: &Assets) -> Result<Vec<u8>, Unwritable> {
        write::write(model, file)
    }

    fn query(model: &Model, query: Query) -> Answer {
        Answer(ops::answer(model, &query))
    }

    fn guide() -> Guide {
        ops::guide()
    }

    fn seals(change: &Change) -> bool {
        match change {
            Change::Worked {
                work: PageWork::Redact { .. },
                ..
            } => true,
            Change::Group { changes } => changes.iter().any(Self::seals),
            _ => false,
        }
    }

    fn describe_place(model: &Model, place: &Value) -> Value {
        let id = place.get("page").and_then(Value::as_str);
        let Some(index) = model
            .pages
            .iter()
            .position(|page| Some(page.id.as_str()) == id)
        else {
            return Value::Null;
        };
        let mut described = json!({ "page": index + 1 });
        if let Some(note) = place.get("note").and_then(Value::as_str) {
            described["note"] = json!(note);
        }
        described
    }
}

#[cfg(test)]
mod tests;
