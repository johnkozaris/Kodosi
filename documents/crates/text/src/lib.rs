//! Tree text and its steps: the same document model and the same step results as `ProseMirror`,
//! so the server's tree stays equal to each view's tree. Positions count UTF-16 units, as a
//! JavaScript string does.

mod map;
mod node;
mod replace;
mod resolved;
mod schema;
mod selection;
mod step;

pub use map::StepMap;
pub use node::{Attrs, Fragment, Mark, Node, Slice};
pub use resolved::ResolvedPos;
pub use schema::{BadSchema, MarkId, NodeId, Schema, SchemaSpec};
pub use selection::Selection;
pub use step::Step;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Failed(pub String);

pub(crate) fn failed<T>(message: impl Into<String>) -> Result<T, Failed> {
    Err(Failed(message.into()))
}
