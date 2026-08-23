use serde_json::Value;

use crate::BoxFuture;
use crate::wire::{CredentialId, MaybeSync};

/// Durable resource → credential ownership, namespaced per provider and
/// caller user. Host-provided over shared persistence — bindings must
/// survive restarts and be visible to every instance.
pub trait BindingStore: MaybeSync {
    fn save<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
        credential: CredentialId,
        summary: Value,
    ) -> BoxFuture<'a, Result<(), StateError>>;
    fn find<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<Option<Binding>, StateError>>;
    fn delete<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(), StateError>>;
    fn list<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        page: Page,
    ) -> BoxFuture<'a, Result<BindingPage, StateError>>;
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub provider_id: i64,
    pub owner_user_id: i64,
    pub kind: String,
    pub id: String,
    pub credential: CredentialId,
    /// Resource metadata for local list synthesis.
    pub summary: Value,
    pub created_at_unix: i64,
}

#[derive(Debug, Clone)]
pub struct Page {
    /// Binding id returned as the previous page's continuation cursor.
    pub cursor: Option<String>,
    /// Maximum number of bindings to return.
    pub limit: u32,
}

#[derive(Debug, Clone)]
pub struct BindingPage {
    /// Bindings in descending `(created_at_unix, id)` order.
    pub items: Vec<Binding>,
    /// Last returned binding id when another page remains.
    pub next_cursor: Option<String>,
}

/// Failures from host-provided surface state.
#[derive(Debug, thiserror::Error)]
#[error("surface state: {0}")]
pub struct StateError(pub String);
