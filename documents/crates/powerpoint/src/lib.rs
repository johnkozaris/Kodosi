//! `PowerPoint` files. Each slide and shape has a stable id, and the text in a shape is a small
//! tree with the same steps as Word. A save changes only the parts of a slide that changed.

mod apply;
mod blank;
mod model;
mod ops;
mod read;
mod write;

use document::{Answer, Assets, Format, Guide, Ordered, Refused, Snapshot, Unreadable, Unwritable};

pub use blank::new_file;
pub use model::{Change, Model, PowerPoint, Query, Shape, ShapeFields, Slide};
pub use ops::Op;

impl Format for PowerPoint {
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
