//! Public traits implemented by [`super::ProviderStore`].
//!
//! These are the narrow public interfaces that external consumers of
//! the engine hit: [`ProviderRegistry`] for read-only lookups,
//! [`ProviderMutator`] for provider/credential upserts and removals,
//! and [`EngineEventSource`] for subscribing to the broadcast channel
//! that emits `EngineEvent`s when state changes. They live here so
//! callers that only need the trait shape (e.g. to mock the store in
//! tests) don't have to pull in the ~1400-line `ProviderStore`
//! implementation.

use serde_json::Value;
use tokio::sync::broadcast;

use gproxy_channel::response::UpstreamError;

use super::types::{CredentialSnapshot, EngineEvent, ProviderSnapshot};

pub trait ProviderRegistry {
    fn get_provider(&self, name: &str) -> Result<Option<ProviderSnapshot>, UpstreamError>;
    fn list_providers(&self) -> Result<Vec<ProviderSnapshot>, UpstreamError>;
    fn get_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError>;
    fn list_credentials(
        &self,
        provider_name: Option<&str>,
    ) -> Result<Vec<CredentialSnapshot>, UpstreamError>;
}

pub trait ProviderMutator {
    fn upsert_provider_json(
        &self,
        config: crate::engine::ProviderConfig,
    ) -> Result<(), UpstreamError>;
    fn remove_provider(&self, name: &str) -> bool;
    fn upsert_credential_json(
        &self,
        provider_name: &str,
        index: Option<usize>,
        credential: Value,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError>;
    fn remove_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError>;
}

pub trait EngineEventSource {
    fn subscribe(&self) -> broadcast::Receiver<EngineEvent>;
}
