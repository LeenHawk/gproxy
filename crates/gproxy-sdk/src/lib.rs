//! The embeddable GPROXY handle.
//!
//! [`Core`] executes; it never writes configuration, never resolves a model
//! name and never learns about a peer's write. This crate is the layer that
//! does: it assembles a [`Core`] out of default implementations, owns the
//! configuration writes that advance `settings.config_revision`, turns a login
//! into a credential row, resolves a model name to providers, and keeps every
//! instance's snapshot in step through the shared cache and a revision poll.
//!
//! Identity — users, API keys, organizations, permissions, subscriptions — is
//! not here. It belongs to the application layer above, which reads the same
//! durable revision through `gproxy_store::load_all_data`.
//!
//! ```no_run
//! # async fn example() -> Result<(), gproxy_sdk::SdkError> {
//! use gproxy_sdk::{Gproxy, GproxyBuilder};
//!
//! let gproxy = GproxyBuilder::sqlite("gproxy.db")
//!     .await?
//!     .master_key([0u8; 32])
//!     .build()
//!     .await?;
//! for channel in gproxy.channels() {
//!     println!("{} ({})", channel.display_name, channel.id);
//! }
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
// wasm32-unknown-unknown is single-threaded and the JS transport handles held
// through `dyn OutboundClient` are thread-bound; shared ownership still goes
// through Arc so the handle's API is identical on every target.
#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

pub mod builder;
pub mod call;
pub mod dto;
mod error;
pub mod handle;
mod ids;
pub mod login;
pub mod manage;
pub mod query;
pub mod resolve;
mod rt;
pub mod session;
pub mod sync;

pub use builder::{GproxyBuilder, LoginTtl};
pub use call::{CallBuilder, ConnectBuilder};
/// How every family asks for a list and answers with one. The rest of the
/// DTOs stay behind `dto::` so a host's own names cannot collide with them.
pub use dto::{ListQuery, Page};
pub use error::{SdkError, SdkResult};
pub use handle::Gproxy;
pub use manage::Scope;
pub use query::Query;
pub use resolve::{Plan, ResolveRequest, RoutingTable, Target};
pub use session::GATEWAY_SESSION_HEADER;
pub use sync::{INVALIDATION_TOPIC, SyncMode};

/// Everything a host needs to name the types the handle exchanges, without
/// depending on the engine crates directly. These are re-exports, not new
/// types: a host that already depends on `gproxy-core` sees the same items.
pub use gproxy_cache::Cache;
#[cfg(feature = "memory")]
pub use gproxy_cache::MemoryCache;
pub use gproxy_channel::{
    BaseChannel, ChannelDescriptor, ChannelRegistry,
    channel::{ChannelCapabilities, ConfigKey, ConfigKeyKind, LoginMode},
    channels,
};
pub use gproxy_client::{ClientPool, OutboundClient};
pub use gproxy_core::{
    AesGcmCodec, BudgetOwner, ConfigRevision, Core, CoreData, CoreError, CoreResult,
    CredentialStatus, CredentialSummary, FetchPolicy, Invalidation, Observer, PlaintextCodec,
    PublicationUrl, RefreshMode, ReloadOutcome, RequestContext, SecretCodec, SessionIdentity,
    SessionSource,
};
pub use gproxy_file::Operator;
pub use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse};
pub use gproxy_store::Store;
