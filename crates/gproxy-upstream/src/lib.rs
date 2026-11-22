//! The account side of an upstream: who the provider is, which credential is
//! used, how a person signs in, how the credential is renewed and what is
//! remembered between requests. Model channels and tool upstreams both build
//! on it; nothing here knows what the upstream is asked to do.
//!
//! Stateless: the host owns storage, callbacks and the outbound client.

mod caller;
mod config;
mod error;
mod kind;
pub mod login;
mod oauth;
pub mod protected_resource;
mod refresh;
mod registry;
mod state;
mod view;

pub use caller::{
    CallerIdentity, CallerRole, CallerUsage, CallerUsageWindow, ResourceBindingRecord,
    ServiceCaller,
};
pub use config::{ConfigKey, ConfigKeyKind};
pub use error::{UpstreamError, UpstreamFuture};
pub use kind::{LoginMode, UpstreamKind};
pub use oauth::{
    AcquiredCredential, AuthorizationCode, AuthorizationRequest, AuthorizationStart,
    DeviceAuthorization, DevicePoll, LoginContext, OAuthAuthorizationCode, OAuthCredential,
    OAuthDeviceCode,
};
pub use refresh::{CredentialRefresh, CredentialUpdate, RefreshContext, RefreshReason};
pub use registry::{RegistryError, UpstreamRegistry};
pub use state::{NoState, UpstreamState};
pub use view::{ConnectionPurpose, CredentialContext, CredentialView, ProviderView};
