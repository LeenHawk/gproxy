//! Flow-local, typed identities used by pairwise protocol transforms.
//!
//! This module deliberately contains no wire types. A converter gives it a
//! source dialect, a role and a logical position, and gets back an immutable
//! target identifier. The same flow can then attach source identifiers that
//! arrive in a later stream event without changing the identifier already
//! emitted to a client.

mod allocator;
mod error;
mod state;
pub(crate) mod tool_alias;
mod types;

pub use allocator::{CallResultLink, IdSyntax, IdentityFlow, IdentityHandle, TargetIdPolicy};
pub use error::{IdentityError, IdentityRoleName};
pub use state::{
    IdentityStateRecord, IdentityStateSnapshot, IdentityStateStore, IdentityTarget,
    LateIdentityFacts, OpaqueField, OpaqueSignature,
};
pub use types::{
    DialectId, IdNamespace, IdentityRole, KnownIdPrefix, OutputItemKind, SourceIdentity,
};
