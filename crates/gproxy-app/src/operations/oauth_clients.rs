//! The registered OAuth client list.
//!
//! Two things are different here from every other family.
//!
//! **The id is given, never generated.** It *is* the `client_id` a third-party
//! binary was built with; the gateway cannot choose it.
//!
//! **There is no delete, only [`OAuthClients::retire`].** A hard delete would
//! drop the row that session history refers to, and — worse — would let the
//! same `client_id` be registered again and inherit the grants the old
//! registration had collected. Retirement soft-deletes the row, disables it,
//! and irreversibly revokes every live grant, every grant's internal key and
//! every issued token, through the store's `retire_many`, which does all of
//! that as one atomic batch.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{Repository, entity::oauth::client};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Select, Set};
use serde_json::Value;

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    AppError, Result,
    dto::{ListQuery, OAuthClientDto, OAuthClientPatch, OAuthClientWrite, Page},
};

pub struct OAuthClients<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> OAuthClients<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> OAuthClients<'_, C> {
    /// Every registered client, retired ones included. A console needs to see
    /// a retired row to explain why a client stopped working.
    pub async fn list(&self, query: ListQuery) -> Result<Page<OAuthClientDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<OAuthClientDto> {
        crud::get(self, id).await
    }

    /// Register a client.
    ///
    /// Re-registering a retired `client_id` is refused rather than silently
    /// reviving it: the grants the previous registration collected are
    /// revoked, and a caller that got a fresh row under the old id would be
    /// entitled to assume otherwise.
    pub async fn create(&self, write: OAuthClientWrite) -> Result<OAuthClientDto> {
        crud::create(self, write).await
    }

    pub async fn update(&self, id: &str, patch: OAuthClientPatch) -> Result<OAuthClientDto> {
        crud::update(self, id, patch).await
    }

    /// Soft-delete the client and revoke everything it holds.
    ///
    /// Two transactions, unavoidably and deliberately in this order. The first
    /// is `retire_many`, which is itself atomic and is where the revocation
    /// happens; the second is an empty revision commit whose only job is to
    /// move `config_revision` and notify the peers, because the client
    /// allowlist is part of `AppData`. If the second fails, the client is
    /// still retired and the peers converge on the revision poll — a window
    /// in which a peer is *stricter* than the database, never looser.
    pub async fn retire(&self, id: &str) -> Result<OAuthClientDto> {
        crud::row::<C, Self>(self, id).await?;
        let retired = self
            .writer
            .store()
            .oauth_clients()
            .retire_many(std::slice::from_ref(&id.to_owned()), crate::now_ms())
            .await?;
        if !retired.first().copied().unwrap_or(false) {
            return Err(AppError::Conflict(format!(
                "oauth client `{id}` is already retired"
            )));
        }
        self.writer
            .commit(Vec::new(), &[Scope::OAuthClients, Scope::Keys])
            .await?;
        crud::get(self, id).await
    }

    /// Registered redirect URIs.
    ///
    /// Absolute only: a relative URI would be resolved against whatever the
    /// issuer happened to be mounted at, which is exactly the ambiguity an
    /// exact-match redirect check exists to remove. Fragments are refused
    /// because RFC 6749 §3.1.2 forbids them, and a `*` anywhere is refused
    /// because this registry does no pattern matching and a caller who wrote
    /// one is assuming it does.
    fn redirect_uris(&self, uris: Vec<String>) -> Result<Value> {
        let mut out = Vec::with_capacity(uris.len());
        for uri in uris {
            let uri = crud::text(&uri, "redirectUris")?;
            if !uri.contains("://") {
                return Err(AppError::invalid(format!(
                    "redirect URI `{uri}` must be absolute"
                )));
            }
            if uri.contains('#') {
                return Err(AppError::invalid(format!(
                    "redirect URI `{uri}` must not carry a fragment"
                )));
            }
            if uri.contains('*') {
                return Err(AppError::invalid(format!(
                    "redirect URI `{uri}` must be exact; wildcards are not matched"
                )));
            }
            if !out.contains(&Value::String(uri.clone())) {
                out.push(Value::String(uri));
            }
        }
        Ok(Value::Array(out))
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for OAuthClients<'_, C> {
    type Entity = client::Entity;
    type Dto = OAuthClientDto;
    type Write = OAuthClientWrite;
    type Patch = OAuthClientPatch;

    const ENTITY: &'static str = "oauth client";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().oauth_clients()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::OAuthClients]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = client::Entity::find();
        if let Some(enabled) = query.enabled {
            select = select.filter(client::Column::Enabled.eq(enabled));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(client::Column::Name.contains(&search));
        }
        select
    }

    async fn build(&self, write: OAuthClientWrite) -> Result<(client::ActiveModel, String)> {
        let id = crud::text(&write.id, "id")?;
        if crud::row::<C, Self>(self, &id).await.is_ok() {
            return Err(AppError::Conflict(format!(
                "oauth client `{id}` is already registered"
            )));
        }
        let row = client::ActiveModel {
            id: Set(id.clone()),
            name: Set(crud::text(&write.name, "name")?),
            redirect_uris: Set(self.redirect_uris(write.redirect_uris)?),
            enabled: Set(write.enabled.unwrap_or(true)),
            deleted_at_ms: Set(None),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &client::Model,
        patch: OAuthClientPatch,
    ) -> Result<client::ActiveModel> {
        if current.deleted_at_ms.is_some() {
            return Err(AppError::Conflict(format!(
                "oauth client `{}` is retired and cannot be changed",
                current.id
            )));
        }
        let mut row = client::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            row.name = Set(crud::text(&name, "name")?);
        }
        if let Some(uris) = patch.redirect_uris {
            row.redirect_uris = Set(self.redirect_uris(uris)?);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}
