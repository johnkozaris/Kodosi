//! PDF files. The view draws pages, and notes and form values are fields. A save appends them as
//! an incremental update, so each byte of the file stays. The server does page work, and a
//! redaction rewrites the file, so the removed words stay nowhere.

mod model;
mod ops;
mod read;
mod redact;
mod write;

use document::{Answer, Assets, Format, Guide, Ordered, Refused, Snapshot, Unreadable, Unwritable};

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
        matches!(
            change,
            Change::Worked {
                work: PageWork::Redact { .. },
                ..
            }
        )
    }
}

#[cfg(test)]
mod tests;
