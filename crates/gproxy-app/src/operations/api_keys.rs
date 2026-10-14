//! Gateway API keys: minting, revealing, rotating and binding them.
//!
//! # The plaintext exists twice
//!
//! A key's text is produced by [`generate_api_key`] and returned by
//! [`ApiKeys::create`] and [`ApiKeys::rotate`]. The row holds
//! `key_hash` — the lowercase hex of its SHA-256 — and nothing else, unless
//! the caller asked for `retainSecret`, in which case a copy sealed by core's
//! [`SecretCodec`](gproxy_core::SecretCodec) goes into `secret` and
//! [`ApiKeys::reveal`] can open it again. Retention is off by default: a key
//! the instance cannot reproduce is a key a database leak does not hand over.
//!
//! The seal is bound to the key's own id, exactly as a credential's is to
//! its credential id, so a sealed blob copied onto another row does not open.
//!
//! # The binding is validated, not trusted
//!
//! `organization_id` and `team_id` decide the budget owner chain, the
//! permission subject and the credential-visibility boundary at once. They are
//! set here and never read from a request header. Three things are checked
//! before a row is written:
//!
//! 1. the organization and the team exist;
//! 2. the key's user is a member of them — a key cannot reach a scope its
//!    holder is not in;
//! 3. when both are set, the team's parent organization *is* that
//!    organization. A team-bound key whose `organization_id` named some other
//!    organization would have a budget chain and a visibility boundary that
//!    disagreed.
//!
//! On the SQLite upgrade path these columns carry no foreign key at all, so
//! these checks are not a nicety in front of the database — for two of the
//! three, they are the only check there is.

use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::{
    Repository,
    entity::{
        identity::api_key::{self, ApiKeyKind},
        limits::quota,
    },
};
use sea_orm::{ActiveValue, ColumnTrait, EntityTrait, QueryFilter, Select, Set};
use serde_json::Value;

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    AppError, Result,
    auth::{API_KEY_PREFIX, generate_api_key},
    dto::{
        ApiKeyBudgetWrite, ApiKeyCreated, ApiKeyDto, ApiKeyPatch, ApiKeySecretDto, ApiKeyWrite,
        ListQuery, Page,
    },
};

pub struct ApiKeys<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> ApiKeys<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> ApiKeys<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<ApiKeyDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<ApiKeyDto> {
        crud::get(self, id).await
    }

    /// Mint a key. The returned `token` is the only copy of the plaintext the
    /// caller will get unless the row retained one.
    pub async fn create(&self, write: ApiKeyWrite) -> Result<ApiKeyCreated> {
        let (token, prefix, key_hash) = generate_api_key(API_KEY_PREFIX)?;
        self.write_row(write, token, prefix, key_hash).await
    }

    /// Write a row for a key whose text the caller already has.
    ///
    /// For provisioning: an operator who keeps the gateway key in a secret
    /// manager hands it here instead of taking a generated one, and the same
    /// value works on every instance of the deployment. The validation, the
    /// binding rules and the digest are the ones [`ApiKeys::create`] uses —
    /// which is the point. A second, hand-rolled insert is how v3 ended up with
    /// a bootstrap key that authenticated nowhere: it wrote the digest of the
    /// key's *payload* while the lookup asked for the digest of the whole text.
    ///
    /// The key's strength is the caller's problem. Nothing here can tell a
    /// 32-byte random token from a short one, so a weak key is accepted exactly
    /// as typed; only a blank one is refused.
    pub async fn adopt(&self, write: ApiKeyWrite, token: &str) -> Result<ApiKeyCreated> {
        let (prefix, key_hash) = crate::auth::adopt_api_key(token)?;
        self.write_row(write, token.trim().to_owned(), prefix, key_hash)
            .await
    }

    /// Write a row for a key the caller has the **digest** of and not the text.
    ///
    /// There is exactly one caller — the v3 migration — and exactly one reason
    /// it cannot use [`ApiKeys::adopt`]: v3 stored `SHA-256(payload)`, the text
    /// with an `sk-`/`at-` presentation prefix removed, and a digest is not
    /// invertible. Carrying those bytes across verbatim is what lets an
    /// operator's existing key keep working, and [`crate::auth::digests`] finds
    /// it on its second rung, which exists for precisely this row.
    ///
    /// Adopting by plaintext instead would be worse than useless here even when
    /// the text happens to be available: it would store `SHA-256("sk-X")` and
    /// so break the `sk-X` / `at-X` / `X` interchangeability that the payload
    /// digest is the whole reason for.
    ///
    /// `prefix` is display only — the first eight characters of the body for a
    /// minted key; for a migrated one, whatever v3 knew, because v3 never
    /// stored the body. `retained` is the plaintext when, and only when, the
    /// source kept a revealable copy; it is sealed exactly as a minted key's
    /// would be, so [`ApiKeys::reveal`] keeps answering after the migration.
    ///
    /// Everything else — the user, the binding, the revision
    /// commit — is [`ApiKeys::create`]'s, because it is the same code.
    pub async fn adopt_digest(
        &self,
        write: ApiKeyWrite,
        digest: &[u8; 32],
        prefix: &str,
        retained: Option<&str>,
    ) -> Result<ApiKeyDto> {
        let prefix = match prefix.trim() {
            "" => return Err(AppError::invalid("api key prefix must not be blank")),
            prefix => prefix.to_owned(),
        };
        self.insert(
            write,
            prefix,
            crate::snapshot::encode_key_hash(digest),
            retained,
        )
        .await
    }

    /// The shared half: validate the binding, build the row, commit one
    /// revision. Both entry points reach the database only through here, so
    /// there is one set of rules and one digest.
    async fn write_row(
        &self,
        write: ApiKeyWrite,
        token: String,
        prefix: String,
        key_hash: String,
    ) -> Result<ApiKeyCreated> {
        let retain = write.retain_secret.unwrap_or(false);
        let key = self
            .insert(write, prefix, key_hash, retain.then_some(token.as_str()))
            .await?;
        Ok(ApiKeyCreated { key, token })
    }

    /// Validate the binding, build the row, commit one revision.
    async fn insert(
        &self,
        write: ApiKeyWrite,
        prefix: String,
        key_hash: String,
        retained: Option<&str>,
    ) -> Result<ApiKeyDto> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let user_id = crud::text(&write.user_id, "userId")?;
        crud::require_rows(
            self.writer.store().users(),
            "user",
            std::slice::from_ref(&user_id),
        )
        .await?;
        let organization_id = crud::optional_text(write.organization_id);
        let team_id = crud::optional_text(write.team_id);
        self.validate_binding(&user_id, organization_id.as_deref(), team_id.as_deref())
            .await?;

        let secret = match retained {
            Some(token) => Some(self.seal(&id, token)?),
            None => None,
        };
        let row = api_key::ActiveModel {
            id: Set(id.clone()),
            user_id: Set(user_id),
            organization_id: Set(organization_id),
            team_id: Set(team_id),
            name: Set(crud::text(&write.name, "name")?),
            // Always `user`: an `oauth` key belongs to a grant and is written
            // by the issuer's atomic authorization, never by this family.
            kind: Set(ApiKeyKind::User),
            key_hash: Set(key_hash),
            prefix: Set(prefix),
            secret: Set(secret),
            expires_at_ms: Set(write.expires_at_ms),
            enabled: Set(write.enabled.unwrap_or(true)),
            management: Set(write.management.unwrap_or(false)),
        };
        let mut statements = vec![BatchStatement::Execute(
            self.writer.store().api_keys().insert_statement(row)?,
        )];
        let has_budget = write.budget.is_some();
        if let Some(budget) = write.budget {
            let budget = initial_budget(&id, budget)?;
            statements.push(BatchStatement::Execute(
                self.writer.store().quotas().insert_statement(budget)?,
            ));
        }
        let key = crud::commit_one::<C, Self>(self, statements, &id, &[Scope::Keys]).await?;
        // A newly returned key must not spend before its initial budget is loaded.
        // Identity publication still belongs to the host, as for other key writes.
        if has_budget {
            self.writer.gproxy().reload().await?;
        }
        Ok(key)
    }

    /// Read back the plaintext of a key that was created with `retainSecret`.
    ///
    /// `Conflict` rather than `NotFound` when nothing was retained: the key
    /// exists, the instance simply never kept a copy, and the answer is to
    /// rotate it rather than to look somewhere else.
    pub async fn reveal(&self, id: &str) -> Result<ApiKeySecretDto> {
        let row = crud::row::<C, Self>(self, id).await?;
        refuse_oauth(&row)?;
        let sealed = row.secret.ok_or_else(|| {
            AppError::Conflict(
                "this key was created without a retained secret; rotate it for a new one".into(),
            )
        })?;
        let opened = self
            .writer
            .gproxy()
            .core()
            .secret_codec()
            .open(id, &sealed)
            .map_err(|error| AppError::internal(format!("api key secret: {error}")))?;
        let token = opened
            .as_str()
            .ok_or_else(|| AppError::internal("api key secret is not a string"))?;
        Ok(ApiKeySecretDto {
            id: row.id,
            token: token.to_owned(),
        })
    }

    /// Replace a key's secret, keeping its id, name and binding.
    ///
    /// The old text stops authenticating the moment the revision lands: the
    /// digest it was found under is overwritten, not added to. A key that had
    /// a retained secret keeps one; a key that did not, does not.
    pub async fn rotate(&self, id: &str) -> Result<ApiKeyCreated> {
        let current = crud::row::<C, Self>(self, id).await?;
        refuse_oauth(&current)?;
        let (token, prefix, key_hash) = generate_api_key(API_KEY_PREFIX)?;
        let mut row = api_key::ActiveModel {
            id: Set(id.to_owned()),
            key_hash: Set(key_hash),
            prefix: Set(prefix),
            ..Default::default()
        };
        if current.secret.is_some() {
            row.secret = Set(Some(self.seal(id, &token)?));
        }
        let statement = self
            .writer
            .store()
            .api_keys()
            .update_statement(row)?
            .ok_or_else(|| AppError::internal("rotation patch set no column"))?;
        let key = crud::commit_one::<C, Self>(
            self,
            vec![BatchStatement::Execute(statement)],
            id,
            &[Scope::Keys],
        )
        .await?;
        Ok(ApiKeyCreated { key, token })
    }

    pub async fn update(&self, id: &str, patch: ApiKeyPatch) -> Result<ApiKeyDto> {
        crud::update(self, id, patch).await
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }

    /// Seal `token` under the key's own id, so the blob cannot be moved to
    /// another row and opened there.
    fn seal(&self, id: &str, token: &str) -> Result<Vec<u8>> {
        self.writer
            .gproxy()
            .core()
            .secret_codec()
            .seal(id, &Value::String(token.to_owned()))
            .map_err(|error| AppError::internal(format!("api key secret: {error}")))
    }

    /// The three binding rules of the module note.
    async fn validate_binding(
        &self,
        user_id: &str,
        organization_id: Option<&str>,
        team_id: Option<&str>,
    ) -> Result<()> {
        if let Some(team_id) = team_id {
            let team = self
                .writer
                .store()
                .teams()
                .get_many(&[team_id.to_owned()])
                .await?
                .into_iter()
                .next()
                .flatten()
                .ok_or_else(|| AppError::not_found("team", team_id))?;
            if let Some(organization_id) = organization_id
                && team.organization_id != organization_id
            {
                return Err(AppError::invalid(format!(
                    "team `{team_id}` belongs to organization `{}`, not `{organization_id}`",
                    team.organization_id
                )));
            }
            self.require_team_member(user_id, team_id).await?;
        }
        if let Some(organization_id) = organization_id {
            crud::require_rows(
                self.writer.store().organizations(),
                "organization",
                std::slice::from_ref(&organization_id.to_owned()),
            )
            .await?;
            self.require_org_member(user_id, organization_id).await?;
        }
        Ok(())
    }

    /// Always the database, never the snapshot.
    ///
    /// The snapshot can be a revision behind in both directions. A miss would
    /// refuse a membership created by the operation just before; a hit is no
    /// safer, because it may be a membership that was just removed. Removal
    /// disables the member's bound keys in its own revision, so a key minted
    /// from a stale positive answer would be the one bound key that removal
    /// never saw — and admission trusts the binding from then on. Minting is
    /// rare, and one primary-key read is the price of never issuing it.
    async fn require_org_member(&self, user_id: &str, organization_id: &str) -> Result<()> {
        let found = self
            .writer
            .store()
            .organization_members()
            .get_many(&[(organization_id.to_owned(), user_id.to_owned())])
            .await?;
        if found.into_iter().flatten().next().is_none() {
            return Err(AppError::invalid(format!(
                "user `{user_id}` is not a member of organization `{organization_id}`"
            )));
        }
        Ok(())
    }

    async fn require_team_member(&self, user_id: &str, team_id: &str) -> Result<()> {
        let found = self
            .writer
            .store()
            .team_members()
            .get_many(&[(team_id.to_owned(), user_id.to_owned())])
            .await?;
        if found.into_iter().flatten().next().is_none() {
            return Err(AppError::invalid(format!(
                "user `{user_id}` is not a member of team `{team_id}`"
            )));
        }
        Ok(())
    }
}

/// An `oauth` key is a grant's internal identity, not a bearer credential.
/// Revealing or rotating one would produce a token that cannot authenticate
/// anyway, and would suggest the grant can be managed as an ordinary key.
fn refuse_oauth(row: &api_key::Model) -> Result<()> {
    if row.kind == ApiKeyKind::OAuth {
        return Err(AppError::Conflict(
            "this key belongs to an OAuth grant; revoke the grant instead".into(),
        ));
    }
    Ok(())
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for ApiKeys<'_, C> {
    type Entity = api_key::Entity;
    type Dto = ApiKeyDto;
    type Write = ApiKeyWrite;
    type Patch = ApiKeyPatch;

    const ENTITY: &'static str = "api key";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().api_keys()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Keys]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = api_key::Entity::find();
        if let Some(user_id) = crud::optional_text(query.user_id.clone()) {
            select = select.filter(api_key::Column::UserId.eq(user_id));
        }
        if let Some(organization_id) = crud::optional_text(query.organization_id.clone()) {
            select = select.filter(api_key::Column::OrganizationId.eq(organization_id));
        }
        if let Some(team_id) = crud::optional_text(query.team_id.clone()) {
            select = select.filter(api_key::Column::TeamId.eq(team_id));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(api_key::Column::Enabled.eq(enabled));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(api_key::Column::Name.contains(&search));
        }
        select
    }

    /// Unreachable: this family never calls `crud::create` or `crud::batch`,
    /// because minting produces a plaintext a generic create has nowhere to
    /// return. [`ApiKeys::create`] builds its own row and commits it through
    /// `crud::commit_one`. Implemented as a refusal rather than as a silent
    /// mint, which would write a key nobody could ever be told.
    async fn build(&self, _write: ApiKeyWrite) -> Result<(api_key::ActiveModel, String)> {
        Err(AppError::internal(
            "api keys are minted through ApiKeys::create",
        ))
    }

    async fn change(
        &self,
        current: &api_key::Model,
        patch: ApiKeyPatch,
    ) -> Result<api_key::ActiveModel> {
        let mut row = api_key::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            row.name = Set(crud::text(&name, "name")?);
        }
        if let Some(organization_id) = patch.organization_id.clone() {
            row.organization_id = Set(crud::optional_text(organization_id));
        }
        if let Some(team_id) = patch.team_id.clone() {
            row.team_id = Set(crud::optional_text(team_id));
        }
        if let Some(expires_at_ms) = patch.expires_at_ms {
            row.expires_at_ms = Set(expires_at_ms);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        if let Some(management) = patch.management {
            row.management = Set(management);
        }
        // Re-validate against the binding the row will actually have, not
        // against the half the patch happened to mention.
        let organization_id = resulting(&row.organization_id, &current.organization_id);
        let team_id = resulting(&row.team_id, &current.team_id);
        self.validate_binding(
            &current.user_id,
            organization_id.as_deref(),
            team_id.as_deref(),
        )
        .await?;
        Ok(row)
    }
}

/// The value a nullable column will hold after the patch: what the patch set,
/// or what the row already had.
fn resulting(patched: &ActiveValue<Option<String>>, current: &Option<String>) -> Option<String> {
    match patched {
        ActiveValue::Set(value) => value.clone(),
        _ => current.clone(),
    }
}

fn initial_budget(key_id: &str, write: ApiKeyBudgetWrite) -> Result<quota::ActiveModel> {
    let row = quota::Model {
        id: crud::id_or_new(None)?,
        owner_kind: "api_key".to_owned(),
        owner_id: key_id.to_owned(),
        window_key: crud::optional_text(write.window_key).unwrap_or_else(|| "primary".to_owned()),
        metric: "cost".to_owned(),
        unit: "USD".to_owned(),
        limit_value: crud::decimal(&write.limit_value, "budget.limitValue")?,
        period: crud::optional_text(write.period).unwrap_or_else(|| "1m".to_owned()),
        period_seconds: write.period_seconds,
        anchor_at_ms: write.anchor_at_ms,
        model_pattern: crud::optional_text(write.model_pattern),
        enabled: true,
    };
    gproxy_core::BudgetData::compile(&row).map_err(AppError::invalid)?;
    Ok(quota::ActiveModel {
        id: Set(row.id),
        owner_kind: Set(row.owner_kind),
        owner_id: Set(row.owner_id),
        window_key: Set(row.window_key),
        metric: Set(row.metric),
        unit: Set(row.unit),
        limit_value: Set(row.limit_value),
        period: Set(row.period),
        period_seconds: Set(row.period_seconds),
        anchor_at_ms: Set(row.anchor_at_ms),
        model_pattern: Set(row.model_pattern),
        enabled: Set(row.enabled),
    })
}
