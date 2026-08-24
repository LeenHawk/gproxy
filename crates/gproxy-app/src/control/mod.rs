mod mutation;
mod snapshot;

pub(crate) use mutation::apply;
pub use mutation::{ControlMutation, MutationResult};
pub(crate) use snapshot::SnapshotControl;
