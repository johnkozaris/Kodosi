//! Documents (.docx). The server keeps a text tree of the body and the original XML of each block, so
//! a save makes new XML only for blocks that changed, and each other byte stays.

mod apply;
mod blank;
mod model;
mod ops;
mod read;
mod styles;
mod write;

use document::{Answer, Assets, Format, Guide, Ordered, Refused, Snapshot, Unreadable, Unwritable};
use serde_json::json;

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
