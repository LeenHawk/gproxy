use gproxy_channel_api::{
    Binding, BindingPage, BindingStore, BoxFuture, CredentialId, Page, StateError,
};

use super::AppHost;

impl BindingStore for AppHost {
    fn save<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
        credential: CredentialId,
        summary: serde_json::Value,
    ) -> BoxFuture<'a, Result<(), StateError>> {
        Box::pin(async move {
            self.services
                .store
                .save_binding(&gproxy_store::records::BindingInput {
                    provider_id,
                    owner_user_id,
                    kind: kind.to_owned(),
                    resource_id: id.to_owned(),
                    credential_id: credential.0,
                    summary,
                })
                .await
                .map_err(state_error)
        })
    }

    fn find<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<Option<Binding>, StateError>> {
        Box::pin(async move {
            self.services
                .store
                .find_binding(provider_id, owner_user_id, kind, id)
                .await
                .map(|binding| binding.map(convert))
                .map_err(state_error)
        })
    }

    fn delete<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(), StateError>> {
        Box::pin(async move {
            self.services
                .store
                .delete_binding(provider_id, owner_user_id, kind, id)
                .await
                .map_err(state_error)
        })
    }

    fn list<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        page: Page,
    ) -> BoxFuture<'a, Result<BindingPage, StateError>> {
        Box::pin(async move {
            let page = self
                .services
                .store
                .list_bindings(
                    provider_id,
                    owner_user_id,
                    kind,
                    page.cursor.as_deref(),
                    page.limit,
                )
                .await
                .map_err(state_error)?;
            Ok(BindingPage {
                items: page.items.into_iter().map(convert).collect(),
                next_cursor: page.next_cursor,
            })
        })
    }
}

fn convert(binding: gproxy_store::records::BindingRecord) -> Binding {
    Binding {
        provider_id: binding.provider_id,
        owner_user_id: binding.owner_user_id,
        kind: binding.kind,
        id: binding.resource_id,
        credential: CredentialId(binding.credential_id),
        summary: binding.summary,
        created_at_unix: binding.created_at,
    }
}

fn state_error(error: gproxy_store::StoreError) -> StateError {
    StateError(error.to_string())
}
