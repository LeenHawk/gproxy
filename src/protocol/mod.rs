//! Protocol wire types and shared protocol taxonomy.

pub mod claude;
pub mod endpoint;
pub mod gemini;
pub mod openai;
pub mod operation;

pub use endpoint::*;
pub use operation::{
    ContentGenerationKind, Endpoint, HttpMethod, Operation, OperationGroup, OperationKey,
    OperationKind, Provider,
};
