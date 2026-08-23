use gproxy_channel_api::{Binding, BindingStore, BoxFuture, Page, StateError};

use super::memory::MemoryHost;
use crate::host::CredentialId;

impl BindingStore for MemoryHost {
    fn save<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
        credential: CredentialId,
        summary: serde_json::Value,
    ) -> BoxFuture<'a, Result<(), StateError>> {
        self.state.lock().expect("state lock").bindings.insert(
            (provider_id, owner_user_id, kind.into(), id.into()),
            Binding {
                provider_id,
                owner_user_id,
                kind: kind.into(),
                id: id.into(),
                credential,
                summary,
                created_at_unix: 0,
            },
        );
        Box::pin(async { Ok(()) })
    }

    fn find<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<Option<Binding>, StateError>> {
        let binding = self
            .state
            .lock()
            .expect("state lock")
            .bindings
            .get(&(provider_id, owner_user_id, kind.into(), id.into()))
            .cloned();
        Box::pin(async move { Ok(binding) })
    }

    fn delete<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(), StateError>> {
        self.state.lock().expect("state lock").bindings.remove(&(
            provider_id,
            owner_user_id,
            kind.into(),
            id.into(),
        ));
        Box::pin(async { Ok(()) })
    }

    fn list<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        page: Page,
    ) -> BoxFuture<'a, Result<Vec<Binding>, StateError>> {
        let bindings = self
            .state
            .lock()
            .expect("state lock")
            .bindings
            .values()
            .filter(|binding| {
                binding.provider_id == provider_id
                    && binding.owner_user_id == owner_user_id
                    && binding.kind == kind
            })
            .take(page.limit as usize)
            .cloned()
            .collect();
        Box::pin(async move { Ok(bindings) })
    }
}
