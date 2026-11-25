//! `ProseMirror`-compatible document tree and step engine so the host tree stays identical to
//! frontend views. Positions are UTF-16 code unit offsets matching JavaScript strings.

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
