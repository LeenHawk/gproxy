//! Process-local live-stream continuations for native operation drivers.

use std::collections::VecDeque;

use bytes::Bytes;
use gproxy_channel_api::{CredentialId, MaybeSync, PreparedRequest};

use crate::boundary::ByteStream;
use crate::control::Target;
use crate::error::StoreError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContinuationKey {
    pub channel: &'static str,
    pub provider_id: i64,
    pub owner_user_id: i64,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationMeta {
    pub credential: CredentialId,
    pub generation: String,
}

pub struct Continuation {
    pub(crate) key: ContinuationKey,
    pub(crate) generation: String,
    pub(crate) target: Target,
    pub(crate) stream: ByteStream,
    pub(crate) pending: VecDeque<Bytes>,
    pub(crate) status: http::StatusCode,
    pub(crate) headers: http::HeaderMap,
    pub(crate) state: serde_json::Value,
    pub(crate) cleanup: PreparedRequest,
    pub(crate) upstream_url: String,
}

impl Continuation {
    pub fn key(&self) -> &ContinuationKey {
        &self.key
    }

    pub fn meta(&self) -> ContinuationMeta {
        ContinuationMeta {
            credential: self.target.credential,
            generation: self.generation.clone(),
        }
    }
}

pub trait ContinuationStore: MaybeSync {
    fn peek(&self, key: &ContinuationKey) -> Result<Option<ContinuationMeta>, StoreError>;
    fn put(
        &self,
        value: Continuation,
    ) -> Result<Option<Continuation>, (StoreError, Box<Continuation>)>;
    fn take(&self, key: &ContinuationKey) -> Result<Option<Continuation>, StoreError>;
    fn take_generation(
        &self,
        key: &ContinuationKey,
        generation: &str,
    ) -> Result<Option<Continuation>, StoreError>;
}
