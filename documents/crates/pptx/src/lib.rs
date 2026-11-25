//! PPTX format support. Slides and shapes use stable OOXML IDs, shape text uses the shared text
//! tree model, and saves patch only modified slide XML.

mod apply;
mod blank;
mod model;
mod ops;
mod read;
mod writable;
mod write;

use document::{Answer, Assets, Format, Guide, Ordered, Refused, Snapshot, Unreadable, Unwritable};
use serde_json::Value;

pub use blank::new_file;
pub use model::{Change, Model, Pptx, Query, Shape, ShapeFields, Slide};
pub use ops::Op;

impl Format for Pptx {
    type Model = Model;
    type Change = Change;
    type Query = Query;

    fn read(file: &[u8], assets: &mut Assets) -> Result<Model, Unreadable> {
        read::read(file, assets)
    }

    fn snapshot(model: &Model) -> Snapshot {
        Snapshot(model.snapshot())
    }

    /// Clients cannot supply raw XML (`keep`, reserved for host external-write merges), and any
    /// fill, line, or stacking order they name must be one the writer can put in the file.
    fn sendable(change: &Change) -> Result<(), String> {
        match change {
            Change::Text { keep, .. } | Change::AddShape { keep, .. } if !keep.is_empty() => {
                Err("Only the server merges a write of another program.".to_owned())
            }
            Change::Shape { fields, .. } => writable::sendable_fields(fields),
            Change::AddShape { shape, .. } => shape
                .get("fill")
                .and_then(Value::as_str)
                .map_or(Ok(()), writable::writable_fill),
            Change::Group { changes } => changes.iter().try_for_each(Self::sendable),
            _ => Ok(()),
        }
    }

    fn apply(
        model: &mut Model,
        change: Change,
        since: &[Ordered<Change>],
        _assets: &mut Assets,
    ) -> Result<Change, Refused> {
        apply::apply(model, change, since)
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
}

#[cfg(test)]
mod tests;
