use crate::protocol::OperationKey;

/// Per-call transform settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransformContext {
    pub source: OperationKey,
    pub target: OperationKey,
}

impl TransformContext {
    pub const fn new(source: OperationKey, target: OperationKey) -> Self {
        Self { source, target }
    }
}
