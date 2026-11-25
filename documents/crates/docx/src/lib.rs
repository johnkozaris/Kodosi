//! DOCX format support. Keeps a text tree of the document body alongside each block's original
//! XML so saves rewrite only modified blocks and preserve all other bytes.

mod apply;
mod blank;
mod model;
mod ops;
mod place;
mod read;
mod styles;
mod writable;
mod write;

use document::{Answer, Assets, Format, Guide, Ordered, Refused, Snapshot, Unreadable, Unwritable};
use serde_json::{Value, json};

pub use blank::new_file;
pub use model::{Change, Docx, Model, Query};
pub use ops::Op;

impl Format for Docx {
    type Model = Model;
    type Change = Change;
    type Query = Query;

    fn read(file: &[u8], assets: &mut Assets) -> Result<Model, Unreadable> {
        read::read(file, assets)
    }

    fn snapshot(model: &Model) -> Snapshot {
        Snapshot(json!({
            "doc": model.doc.to_json(model::schema()),
            "view": model.view,
        }))
    }

    /// Clients may only send steps (without raw XML payloads) or named ops; `keep`, `sources`, and
    /// `Merge` are host-only for external-write merges.
    fn sendable(change: &Change) -> Result<(), String> {
        match change {
            Change::Steps { keep, sources, .. } if keep.is_empty() && sources.is_empty() => Ok(()),
            Change::Ops { .. } => Ok(()),
            Change::Steps { .. } | Change::Merge { .. } => {
                Err("Only the server merges a write of another program.".to_owned())
            }
        }
    }

    fn apply(
        model: &mut Model,
        change: Change,
        since: &[Ordered<Change>],
        assets: &mut Assets,
    ) -> Result<Change, Refused> {
        apply::apply(model, change, since, assets)
    }

    fn compare(from: &Model, to: &Model) -> Vec<Change> {
        apply::compare(from, to)
    }

    fn write(model: &Model, file: &[u8], assets: &Assets) -> Result<Vec<u8>, Unwritable> {
        write::write(model, file, assets)
    }

    fn query(model: &Model, query: Query) -> Answer {
        Answer(ops::answer(model, &query))
    }

    fn guide() -> Guide {
        ops::guide()
    }

    fn map_place(place: &mut Value, change: &Change) {
        place::map(place, change);
    }

    fn describe_place(model: &Model, place: &Value) -> Value {
        place::describe(&model.doc, place)
    }
}

#[cfg(test)]
mod tests;
