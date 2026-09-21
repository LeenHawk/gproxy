//! The caller's own gateway keys.
//!
//! Five operations, and the same sentence in front of four of them: **read the
//! row, check it is yours, then delegate**. The check answers `NotFound` for a
//! key that belongs to somebody else, which makes another user's key
//! indistinguishable from an id that was never minted — see the module note.
//!
//! The write itself is [`ApiKeys`](crate::operations::ApiKeys)'. Minting,
//! sealing, rotating and the three binding rules are validated there, once;
//! what this file adds is that the row's user is the caller and that an
//! `oauth` key is invisible here.
//!
//! # Why `oauth` keys are not listed
//!
//! Every OAuth grant owns an internal `api_keys` row. It is not a bearer
//! credential — authentication refuses it when it is presented as one — and it
//! cannot be rotated or revealed into anything that would work. Showing it
//! would offer the user a key they cannot use and a delete that would silently
//! break an authorization they would then have to hunt for under "sessions".
//! The way to end one is [`PortalOAuthSessions::revoke`].
//!
//! [`PortalOAuthSessions::revoke`]: super::PortalOAuthSessions::revoke

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::api_key::{self, ApiKeyKind};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use super::Portal;
use crate::{
    AppConfig, AppError, Caller, Result,
    dto::{ApiKeyWrite, PortalKeyCreate, PortalKeyCreated, PortalKeyDto, PortalKeySecretDto},
    operations::Writer,
};

pub struct PortalKeys<'a, C> {
    portal: Portal<'a, C>,
}

impl<'a, C> PortalKeys<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>, config: &'a AppConfig, caller: Caller) -> Self {
        Self {
            portal: Portal::new(writer, config, caller),
        }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> PortalKeys<'_, C> {
    /// The caller's own `user` keys, expired and disabled ones included: a
    /// portal has to be able to explain a key that stopped working.
    pub async fn list(&self) -> Result<Vec<PortalKeyDto>> {
        let rows = self
            .portal
            .writer()
            .store()
            .api_keys()
            .query(
                api_key::Entity::find()
                    .filter(api_key::Column::UserId.eq(&self.portal.caller().user_id))
                    .filter(api_key::Column::Kind.eq(ApiKeyKind::User)),
            )
            .await?;
        Ok(rows.into_iter().map(PortalKeyDto::from).collect())
    }

    /// Mint a key for the caller.
    ///
    /// The user id is the caller's and is not a field of the request. The
    /// organization and team, if named, go through
    /// [`ApiKeys::create`](crate::operations::ApiKeys::create), which refuses a
    /// scope the caller is not a member of — so a portal user cannot bind a
    /// key to an organization they do not belong to, and this file does not
    /// re-implement that rule.
    pub async fn create(&self, write: PortalKeyCreate) -> Result<PortalKeyCreated> {
        self.portal.require_person("mint a gateway key")?;
        let created = self
            .portal
            .operations()
            .api_keys()
            .create(ApiKeyWrite {
                id: None,
                user_id: self.portal.caller().user_id.clone(),
                name: write.name,
                organization_id: write.organization_id,
                team_id: write.team_id,
                // Not offered: a subscription is issued to a user by an
                // operator, and which one a key spends is part of that
                // decision rather than a self-service choice.
                subscription_id: None,
                expires_at_ms: write.expires_at_ms,
                enabled: None,
                retain_secret: write.retain_secret,
            })
            .await?;
        Ok(PortalKeyCreated {
            key: created.key.into(),
            token: created.token,
        })
    }

    /// Replace one of the caller's own keys' secrets, keeping its id, name and
    /// binding. The old text stops authenticating the moment the revision
    /// lands.
    pub async fn rotate(&self, id: &str) -> Result<PortalKeyCreated> {
        self.portal.require_person("rotate a gateway key")?;
        self.own(id).await?;
        let created = self.portal.operations().api_keys().rotate(id).await?;
        Ok(PortalKeyCreated {
            key: created.key.into(),
            token: created.token,
        })
    }

    /// Delete one of the caller's own keys.
    pub async fn delete(&self, id: &str) -> Result<()> {
        self.portal.require_person("delete a gateway key")?;
        self.own(id).await?;
        self.portal.operations().api_keys().delete(id).await
    }

    /// Read back the plaintext of one of the caller's own keys, for a key that
    /// was created with `retainSecret`.
    ///
    /// A key that retained nothing answers `Conflict` and not `NotFound`: the
    /// key exists and is the caller's, the instance simply never kept a copy,
    /// and the thing to do about it is to rotate.
    pub async fn reveal(&self, id: &str) -> Result<PortalKeySecretDto> {
        self.portal.require_person("reveal a gateway key")?;
        self.own(id).await?;
        let secret = self.portal.operations().api_keys().reveal(id).await?;
        Ok(PortalKeySecretDto {
            id: secret.id,
            token: secret.token,
        })
    }

    /// That `id` names a live `user` key of the caller's.
    ///
    /// Read from the database and not from the snapshot: the snapshot can be a
    /// revision behind, and a key created a moment ago would then look like
    /// somebody else's. Three different facts — no such key, another user's
    /// key, a grant's internal key — collapse into one `NotFound`, because the
    /// caller is entitled to learn the same thing from all three: there is no
    /// key of yours here.
    async fn own(&self, id: &str) -> Result<api_key::Model> {
        let row = self
            .portal
            .writer()
            .store()
            .api_keys()
            .get_many(&[id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
            .filter(|row| {
                row.user_id == self.portal.caller().user_id && row.kind == ApiKeyKind::User
            });
        row.ok_or_else(|| AppError::not_found("api key", id))
    }
}
