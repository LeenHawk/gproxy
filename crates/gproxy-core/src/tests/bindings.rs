use gproxy_channel_api::{Binding, BindingPage, BindingStore, BoxFuture, Page, StateError};

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
    ) -> BoxFuture<'a, Result<BindingPage, StateError>> {
        let mut bindings = self
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
            .cloned()
            .collect::<Vec<_>>();
        bindings.sort_by(|left, right| {
            right
                .created_at_unix
                .cmp(&left.created_at_unix)
                .then_with(|| right.id.cmp(&left.id))
        });
        let start = match page.cursor {
            Some(cursor) => match bindings.iter().position(|binding| binding.id == cursor) {
                Some(index) => index + 1,
                None => {
                    return Box::pin(async move {
                        Err(StateError(format!("unknown binding cursor `{cursor}`")))
                    });
                }
            },
            None => 0,
        };
        let limit = page.limit as usize;
        let mut items = bindings
            .into_iter()
            .skip(start)
            .take(limit.saturating_add(1))
            .collect::<Vec<_>>();
        let has_more = items.len() > limit;
        if has_more {
            items.pop();
        }
        let next_cursor = if has_more {
            items.last().map(|binding| binding.id.clone())
        } else {
            None
        };
        Box::pin(async move { Ok(BindingPage { items, next_cursor }) })
    }
}

#[test]
fn memory_binding_pages_use_stable_exclusive_cursors() {
    let host = MemoryHost::new(false);
    for id in ["older", "newer-a", "newer-b"] {
        super::block_on(host.save(
            3,
            1,
            "file",
            id,
            CredentialId(7),
            serde_json::json!({"id": id}),
        ))
        .expect("save binding");
    }
    {
        let mut state = host.state.lock().expect("state lock");
        state
            .bindings
            .get_mut(&(3, 1, "file".into(), "older".into()))
            .expect("older binding")
            .created_at_unix = 1;
        for id in ["newer-a", "newer-b"] {
            state
                .bindings
                .get_mut(&(3, 1, "file".into(), id.into()))
                .expect("newer binding")
                .created_at_unix = 2;
        }
    }

    let first = super::block_on(host.list(
        3,
        1,
        "file",
        Page {
            cursor: None,
            limit: 2,
        },
    ))
    .expect("first binding page");
    assert_eq!(
        first
            .items
            .iter()
            .map(|binding| binding.id.as_str())
            .collect::<Vec<_>>(),
        ["newer-b", "newer-a"]
    );
    assert_eq!(first.next_cursor.as_deref(), Some("newer-a"));

    let second = super::block_on(host.list(
        3,
        1,
        "file",
        Page {
            cursor: first.next_cursor,
            limit: 2,
        },
    ))
    .expect("second binding page");
    assert_eq!(second.items[0].id, "older");
    assert!(second.next_cursor.is_none());
}
