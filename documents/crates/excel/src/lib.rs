//! Excel files. The server keeps each sheet's cells, and a save swaps only the cells that
//! changed. Each side works out formula results for itself: the view for what it shows, and
//! the server for the values that the file holds.

mod apply;
mod formula;
mod model;
mod ops;
mod read;
mod write;

use document::{Answer, Assets, Format, Guide, Ordered, Refused, Snapshot, Unreadable, Unwritable};

pub use model::{Cell, CellChange, Change, Excel, Model, Query, Value};
pub use ops::Op;

impl Format for Excel {
    type Model = Model;
    type Change = Change;
    type Query = Query;

    fn read(file: &[u8], _assets: &mut Assets) -> Result<Model, Unreadable> {
        read::read(file)
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

    fn write(model: &Model, file: &[u8], _assets: &Assets) -> Result<Vec<u8>, Unwritable> {
        write::write(model, file)
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
