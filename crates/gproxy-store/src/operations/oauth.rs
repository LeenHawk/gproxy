use super::{CasOutcome, cas, sql::insert_if};
use crate::{
    Repository, Result, StoreError,
    entity::{
        identity::{api_key, user},
        oauth::{client, code, device, grant, token},
        subscription::{plan, pool, user_subscription},
    },
    error::{invalid, receipt},
    repository::affected,
};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement, SelectProjection};
use sea_orm::sea_query::{Alias, Expr, ExprTrait, Query, SelectStatement};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, QueryFilter, QueryTrait, SelectTwoRequiredModel,
    SelectorTrait, Set,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub enum ExchangeSource {
    Code {
        id: String,
        hash: Vec<u8>,
        redirect_uri: String,
        code_challenge: String,
    },
    Refresh {
        id: String,
        hash: Vec<u8>,
    },
}
#[derive(Clone, Debug)]
pub struct IssuedToken {
    pub id: String,
    pub hash: Vec<u8>,
    pub expires_at_ms: i64,
}
#[derive(Clone, Debug)]
pub struct TokenExchange {
    pub source: ExchangeSource,
    pub grant_id: String,
    pub client_id: String,
    pub access: IssuedToken,
    pub refresh: IssuedToken,
    pub now_ms: i64,
}
#[derive(Clone, Debug)]
pub struct AccessIdentity {
    pub grant: grant::Model,
    pub token_id: String,
    pub expires_at_ms: i64,
}

fn column(name: &str) -> Expr {
    Expr::col(Alias::new(name))
}
fn equality<
    A: sea_orm::Iden + Copy,
    B: sea_orm::Iden + Copy,
    C: sea_orm::Iden + Copy,
    D: sea_orm::Iden + Copy,
>(
    a: (A, B),
    b: (C, D),
) -> Expr {
    Expr::col(a).equals(b)
}

pub(crate) fn eligible_subscriptions(now: i64) -> SelectStatement {
    Query::select()
        .column((user_subscription::Entity, user_subscription::Column::Id))
        .from(user_subscription::Entity)
        .inner_join(
            plan::Entity,
            equality(
                (user_subscription::Entity, user_subscription::Column::PlanId),
                (plan::Entity, plan::Column::Id),
            ),
        )
        .inner_join(
            pool::Entity,
            equality(
                (plan::Entity, plan::Column::PoolId),
                (pool::Entity, pool::Column::Id),
            ),
        )
        .and_where(user_subscription::Column::Enabled.eq(true))
        .and_where(user_subscription::Column::StartsAtMs.lte(now))
        .cond_where(
            Condition::any()
                .add(user_subscription::Column::ExpiresAtMs.is_null())
                .add(user_subscription::Column::ExpiresAtMs.gt(now)),
        )
        .and_where(pool::Column::Enabled.eq(true))
        .to_owned()
}

fn live_grants(now: i64, selected_client: Option<&str>) -> SelectStatement {
    let mut subscriptions = eligible_subscriptions(now);
    subscriptions.and_where(equality(
        (user_subscription::Entity, user_subscription::Column::UserId),
        (grant::Entity, grant::Column::UserId),
    ));
    let mut query = Query::select();
    query
        .column((grant::Entity, grant::Column::Id))
        .from(grant::Entity)
        .inner_join(
            client::Entity,
            equality(
                (grant::Entity, grant::Column::ClientId),
                (client::Entity, client::Column::Id),
            ),
        )
        .inner_join(
            user::Entity,
            equality(
                (grant::Entity, grant::Column::UserId),
                (user::Entity, user::Column::Id),
            ),
        )
        .inner_join(
            api_key::Entity,
            equality(
                (grant::Entity, grant::Column::ApiKeyId),
                (api_key::Entity, api_key::Column::Id),
            ),
        )
        .and_where(grant::Column::RevokedAtMs.is_null())
        .and_where(client::Column::Enabled.eq(true))
        .and_where(client::Column::DeletedAtMs.is_null())
        .and_where(user::Column::Enabled.eq(true))
        .and_where(api_key::Column::Enabled.eq(true))
        .and_where(api_key::Column::Kind.eq(api_key::ApiKeyKind::OAuth))
        .and_where(equality(
            (api_key::Entity, api_key::Column::UserId),
            (grant::Entity, grant::Column::UserId),
        ))
        .cond_where(
            Condition::any()
                .add(api_key::Column::ExpiresAtMs.is_null())
                .add(api_key::Column::ExpiresAtMs.gt(now)),
        )
        .cond_where(
            Condition::any()
                .add(api_key::Column::SubscriptionId.is_null())
                .add(api_key::Column::SubscriptionId.in_subquery(subscriptions)),
        );
    if let Some(id) = selected_client {
        query.and_where(grant::Column::ClientId.eq(id));
    }
    query.to_owned()
}

impl<C: BatchConnectionTrait> Repository<'_, C, grant::Entity> {
    /// Code/refresh consumption, both token inserts and session statistics are
    /// one atomic batch. Each attempt uses a new receipt so replay cannot rotate
    /// twice, even when the caller supplies the same replacement token IDs.
    pub async fn exchange_tokens_many(
        &self,
        exchanges: Vec<TokenExchange>,
    ) -> Result<Vec<CasOutcome>> {
        let backend = self.db.get_database_backend();
        let mut batch = Vec::new();
        let clients = exchanges
            .iter()
            .map(|e| e.client_id.as_str())
            .collect::<BTreeSet<_>>();
        let grants = exchanges
            .iter()
            .map(|e| e.grant_id.as_str())
            .collect::<BTreeSet<_>>();
        for id in clients {
            batch.push(BatchStatement::Execute(
                client::Entity::update_many()
                    .col_expr(client::Column::Enabled, Expr::col(client::Column::Enabled))
                    .filter(client::Column::Id.eq(id))
                    .build(backend),
            ));
        }
        for id in grants {
            batch.push(BatchStatement::Execute(
                grant::Entity::update_many()
                    .col_expr(
                        grant::Column::RevokedAtMs,
                        Expr::col(grant::Column::RevokedAtMs),
                    )
                    .filter(grant::Column::Id.eq(id))
                    .build(backend),
            ));
        }
        let locks = batch.len();
        let count = exchanges.len();
        for exchange in exchanges {
            for issued in [&exchange.access, &exchange.refresh] {
                if issued.hash.len() != 32 || issued.expires_at_ms <= exchange.now_ms {
                    return Err(invalid(
                        "issued token needs a 32-byte hash and future expiry",
                    ));
                }
            }
            let nonce = receipt()?;
            let (table, id, hash, initial) = match &exchange.source {
                ExchangeSource::Code { id, hash, .. } => {
                    (Alias::new("oauth_codes"), id, hash, true)
                }
                ExchangeSource::Refresh { id, hash } => {
                    (Alias::new("oauth_tokens"), id, hash, false)
                }
            };
            if hash.len() != 32 {
                return Err(invalid("source token hash must be 32 bytes"));
            }
            let mut consume = Query::update();
            consume
                .table(table.clone())
                .value(Alias::new("consumed_at_ms"), exchange.now_ms)
                .value(Alias::new("consumed_by"), nonce.clone())
                .and_where(column("id").eq(id.clone()))
                .and_where(column("grant_id").eq(exchange.grant_id.clone()))
                .and_where(column("consumed_at_ms").is_null())
                .and_where(column("expires_at_ms").gt(exchange.now_ms))
                .and_where(
                    column("grant_id")
                        .in_subquery(live_grants(exchange.now_ms, Some(&exchange.client_id))),
                );
            match &exchange.source {
                ExchangeSource::Code {
                    redirect_uri,
                    code_challenge,
                    ..
                } => {
                    consume
                        .and_where(column("code_hash").eq(hash.clone()))
                        .and_where(column("redirect_uri").eq(redirect_uri.clone()))
                        .and_where(column("code_challenge").eq(code_challenge.clone()));
                }
                ExchangeSource::Refresh { .. } => {
                    consume
                        .and_where(column("token_hash").eq(hash.clone()))
                        .and_where(column("kind").eq("refresh"))
                        .and_where(column("revoked_at_ms").is_null());
                }
            }
            batch.push(BatchStatement::Execute(backend.build(&consume)));
            let proof = Query::select()
                .expr(Expr::val(1))
                .from(table)
                .and_where(column("id").eq(id.clone()))
                .and_where(column("consumed_by").eq(nonce))
                .to_owned();
            for (issued, kind) in [
                (&exchange.access, token::TokenKind::Access),
                (&exchange.refresh, token::TokenKind::Refresh),
            ] {
                let insert = insert_if::<token::Entity>(
                    token::ActiveModel {
                        id: Set(issued.id.clone()),
                        token_hash: Set(issued.hash.clone()),
                        grant_id: Set(exchange.grant_id.clone()),
                        kind: Set(kind),
                        created_at_ms: Set(exchange.now_ms),
                        expires_at_ms: Set(issued.expires_at_ms),
                        ..Default::default()
                    },
                    Condition::all().add(Expr::exists(proof.clone())),
                )?;
                batch.push(BatchStatement::Execute(backend.build(&insert)));
            }
            let mut update = grant::Entity::update_many().col_expr(
                grant::Column::RefreshExpiresAtMs,
                Expr::val(exchange.refresh.expires_at_ms),
            );
            update = if initial {
                update
                    .col_expr(grant::Column::LoggedInAtMs, Expr::val(exchange.now_ms))
                    .col_expr(grant::Column::RefreshCount, Expr::val(0))
            } else {
                update
                    .col_expr(grant::Column::LastRefreshedAtMs, Expr::val(exchange.now_ms))
                    .col_expr(
                        grant::Column::RefreshCount,
                        Expr::col(grant::Column::RefreshCount).add(1),
                    )
            };
            batch.push(BatchStatement::Execute(
                update
                    .filter(grant::Column::Id.eq(exchange.grant_id))
                    .filter(Expr::exists(proof))
                    .build(backend),
            ));
        }
        let mut results = self.db.batch(&batch).await?.into_iter().skip(locks);
        (0..count)
            .map(|_| {
                let outcome = cas(affected(
                    results.next().ok_or(StoreError::UnexpectedResult)?,
                )?);
                for _ in 0..3 {
                    results.next().ok_or(StoreError::UnexpectedResult)?;
                }
                Ok(outcome)
            })
            .collect()
    }

    pub async fn resolve_access_many(
        &self,
        hashes: &[Vec<u8>],
        now: i64,
    ) -> Result<Vec<Option<AccessIdentity>>> {
        let queries = hashes
            .iter()
            .map(|hash| {
                if hash.len() != 32 {
                    return Err(invalid("access hash must be 32 bytes"));
                }
                Ok(token::Entity::find()
                    .filter(token::Column::TokenHash.eq(hash.clone()))
                    .filter(token::Column::Kind.eq(token::TokenKind::Access))
                    .filter(token::Column::ExpiresAtMs.gt(now))
                    .filter(token::Column::RevokedAtMs.is_null())
                    .filter(token::Column::GrantId.in_subquery(live_grants(now, None)))
                    .find_both_related(grant::Entity)
                    .batch_query(self.db.get_database_backend())?)
            })
            .collect::<Result<Vec<_>>>()?;
        self.db.query_batch(&queries).await?.into_iter().map(|rows|rows.into_iter().next().map(|row|{
            let (token,grant)=SelectTwoRequiredModel::<token::Model,grant::Model>::from_raw_query_result(row)?;
            Ok(AccessIdentity{grant,token_id:token.id,expires_at_ms:token.expires_at_ms})
        }).transpose()).collect()
    }

    /// Revocation is idempotent and keeps historical rows; it also disables the
    /// grant's internal API key and marks issued tokens revoked.
    pub async fn revoke_many(&self, ids: &[String], now: i64) -> Result<Vec<bool>> {
        let backend = self.db.get_database_backend();
        let mut batch = Vec::new();
        for id in ids {
            batch.push(
                grant::Entity::update_many()
                    .col_expr(grant::Column::RevokedAtMs, Expr::val(now))
                    .filter(grant::Column::Id.eq(id))
                    .filter(grant::Column::RevokedAtMs.is_null())
                    .build(backend),
            );
            let key = Query::select()
                .column(grant::Column::ApiKeyId)
                .from(grant::Entity)
                .and_where(grant::Column::Id.eq(id))
                .to_owned();
            batch.push(
                api_key::Entity::update_many()
                    .col_expr(api_key::Column::Enabled, Expr::val(false))
                    .filter(api_key::Column::Id.in_subquery(key))
                    .build(backend),
            );
            batch.push(
                token::Entity::update_many()
                    .col_expr(token::Column::RevokedAtMs, Expr::val(now))
                    .filter(token::Column::GrantId.eq(id))
                    .filter(token::Column::RevokedAtMs.is_null())
                    .build(backend),
            );
        }
        Ok(self
            .db
            .atomic_batch(&batch)
            .await?
            .iter()
            .step_by(3)
            .map(|r| r.rows_affected() > 0)
            .collect())
    }
}

/// Already-approved consent. PKCE/scopes/redirect policy is validated by the
/// issuer; storage checks ownership, liveness and atomic creation dependencies.
pub struct Authorization {
    pub api_key: api_key::ActiveModel,
    pub grant: grant::ActiveModel,
    pub code: code::ActiveModel,
    pub device: Option<DeviceApproval>,
    pub now_ms: i64,
}
pub struct DeviceApproval {
    pub id: String,
    pub authorization_payload: Option<Vec<u8>>,
}

impl<C: BatchConnectionTrait> Repository<'_, C, grant::Entity> {
    /// Create an internal key, grant and authorization code together. With a
    /// device ID, reserve and approve that pending flow in the same transaction.
    pub async fn issue_many(&self, authorizations: Vec<Authorization>) -> Result<Vec<CasOutcome>> {
        let backend = self.db.get_database_backend();
        let mut batch = Vec::new();
        let mut indices = Vec::new();
        let clients = authorizations
            .iter()
            .map(|a| {
                a.grant
                    .client_id
                    .try_as_ref()
                    .cloned()
                    .ok_or_else(|| invalid("grant client is required"))
            })
            .collect::<Result<BTreeSet<_>>>()?;
        for id in clients {
            batch.push(BatchStatement::Execute(
                client::Entity::update_many()
                    .col_expr(client::Column::Enabled, Expr::col(client::Column::Enabled))
                    .filter(client::Column::Id.eq(id))
                    .build(backend),
            ));
        }
        for auth in authorizations {
            let key_id = crate::repository::active_key::<api_key::Entity>(&auth.api_key)?;
            let grant_id = crate::repository::active_key::<grant::Entity>(&auth.grant)?;
            crate::repository::active_key::<code::Entity>(&auth.code)?;
            let user_id = auth
                .grant
                .user_id
                .try_as_ref()
                .cloned()
                .ok_or_else(|| invalid("grant user is required"))?;
            let client_id = auth
                .grant
                .client_id
                .try_as_ref()
                .cloned()
                .ok_or_else(|| invalid("grant client is required"))?;
            if auth.api_key.user_id.try_as_ref().cloned() != Some(user_id.clone())
                || auth.api_key.kind.try_as_ref().cloned() != Some(api_key::ApiKeyKind::OAuth)
                || auth.grant.api_key_id.try_as_ref().cloned() != Some(key_id.clone())
                || auth.code.grant_id.try_as_ref().cloned() != Some(grant_id.clone())
            {
                return Err(invalid("authorization key/grant/code ownership mismatch"));
            }
            if auth
                .code
                .code_hash
                .try_as_ref()
                .cloned()
                .is_none_or(|v| v.len() != 32)
                || auth
                    .code
                    .expires_at_ms
                    .try_as_ref()
                    .cloned()
                    .is_none_or(|v| v <= auth.now_ms)
            {
                return Err(invalid("authorization code needs a hash and future expiry"));
            }
            let client = client::Entity::find_by_id(client_id.clone())
                .filter(client::Column::Enabled.eq(true))
                .filter(client::Column::DeletedAtMs.is_null())
                .into_query();
            let user = user::Entity::find_by_id(user_id.clone())
                .filter(user::Column::Enabled.eq(true))
                .into_query();
            let mut allowed = Condition::all()
                .add(Expr::exists(client))
                .add(Expr::exists(user));
            if let Some(Some(subscription)) = auth.api_key.subscription_id.try_as_ref().cloned() {
                let mut eligible = eligible_subscriptions(auth.now_ms);
                eligible
                    .and_where(user_subscription::Column::Id.eq(subscription))
                    .and_where(user_subscription::Column::UserId.eq(user_id));
                allowed = allowed.add(Expr::exists(eligible));
            }
            let receipt = receipt()?;
            indices.push(batch.len());
            if let Some(device) = &auth.device {
                batch.push(BatchStatement::Execute(
                    device::Entity::update_many()
                        .col_expr(device::Column::ApprovalReceipt, Expr::val(receipt.clone()))
                        .filter(device::Column::Id.eq(&device.id))
                        .filter(device::Column::ClientId.eq(&client_id))
                        .filter(device::Column::ApprovedAtMs.is_null())
                        .filter(device::Column::DeniedAtMs.is_null())
                        .filter(device::Column::ConsumedAtMs.is_null())
                        .filter(device::Column::ApprovalReceipt.is_null())
                        .filter(device::Column::ExpiresAtMs.gt(auth.now_ms))
                        .filter(allowed.clone())
                        .build(backend),
                ));
                allowed = allowed.add(Expr::exists(
                    device::Entity::find_by_id(device.id.clone())
                        .filter(device::Column::ApprovalReceipt.eq(receipt.clone()))
                        .into_query(),
                ));
            }
            let key = insert_if::<api_key::Entity>(auth.api_key, allowed.clone())?;
            batch.push(BatchStatement::Execute(backend.build(&key)));
            let key_exists = api_key::Entity::find_by_id(key_id).into_query();
            let grant = insert_if::<grant::Entity>(
                auth.grant,
                allowed.clone().add(Expr::exists(key_exists)),
            )?;
            batch.push(BatchStatement::Execute(backend.build(&grant)));
            let exists = grant::Entity::find_by_id(grant_id.clone()).into_query();
            let code =
                insert_if::<code::Entity>(auth.code, allowed.clone().add(Expr::exists(exists)))?;
            batch.push(BatchStatement::Execute(backend.build(&code)));
            if let Some(device) = auth.device {
                batch.push(BatchStatement::Execute(
                    device::Entity::update_many()
                        .col_expr(device::Column::GrantId, Expr::val(grant_id))
                        .col_expr(device::Column::ApprovedAtMs, Expr::val(auth.now_ms))
                        .col_expr(
                            device::Column::AuthorizationPayload,
                            Expr::val(device.authorization_payload),
                        )
                        .filter(device::Column::Id.eq(device.id))
                        .filter(device::Column::ApprovalReceipt.eq(receipt))
                        .build(backend),
                ));
            }
        }
        let results = self.db.batch(&batch).await?;
        indices
            .into_iter()
            .map(|index| match results.get(index) {
                Some(gproxy_seaorm::BatchResult::Executed(result)) => {
                    Ok(cas(result.rows_affected()))
                }
                _ => Err(StoreError::UnexpectedResult),
            })
            .collect()
    }
}

impl<C: BatchConnectionTrait> Repository<'_, C, device::Entity> {
    pub async fn deny_pending_many(&self, ids: &[String], now: i64) -> Result<Vec<CasOutcome>> {
        let statements = ids
            .iter()
            .map(|id| {
                device::Entity::update_many()
                    .col_expr(device::Column::DeniedAtMs, Expr::val(now))
                    .filter(device::Column::Id.eq(id))
                    .filter(device::Column::ApprovedAtMs.is_null())
                    .filter(device::Column::DeniedAtMs.is_null())
                    .filter(device::Column::ConsumedAtMs.is_null())
                    .filter(device::Column::ApprovalReceipt.is_null())
                    .build(self.db.get_database_backend())
            })
            .collect::<Vec<_>>();
        Ok(self
            .db
            .atomic_batch(&statements)
            .await?
            .iter()
            .map(|r| cas(r.rows_affected()))
            .collect())
    }
}

impl<C: BatchConnectionTrait> Repository<'_, C, client::Entity> {
    /// Soft-delete a client and irreversibly revoke existing authorizations.
    /// Later re-registration cannot resurrect these grants.
    pub async fn retire_many(&self, ids: &[String], now: i64) -> Result<Vec<bool>> {
        let backend = self.db.get_database_backend();
        let mut statements = Vec::new();
        let mut ordered = ids.to_vec();
        ordered.sort();
        ordered.dedup();
        for id in &ordered {
            statements.push(
                client::Entity::update_many()
                    .col_expr(client::Column::Enabled, Expr::val(false))
                    .col_expr(client::Column::DeletedAtMs, Expr::val(now))
                    .filter(client::Column::Id.eq(id))
                    .build(backend),
            );
            statements.push(
                grant::Entity::update_many()
                    .col_expr(grant::Column::RevokedAtMs, Expr::val(now))
                    .filter(grant::Column::ClientId.eq(id))
                    .filter(grant::Column::RevokedAtMs.is_null())
                    .build(backend),
            );
            let grants = Query::select()
                .column(grant::Column::Id)
                .from(grant::Entity)
                .and_where(grant::Column::ClientId.eq(id))
                .to_owned();
            let keys = Query::select()
                .column(grant::Column::ApiKeyId)
                .from(grant::Entity)
                .and_where(grant::Column::ClientId.eq(id))
                .to_owned();
            statements.push(
                api_key::Entity::update_many()
                    .col_expr(api_key::Column::Enabled, Expr::val(false))
                    .filter(api_key::Column::Id.in_subquery(keys))
                    .build(backend),
            );
            statements.push(
                token::Entity::update_many()
                    .col_expr(token::Column::RevokedAtMs, Expr::val(now))
                    .filter(token::Column::GrantId.in_subquery(grants))
                    .filter(token::Column::RevokedAtMs.is_null())
                    .build(backend),
            );
        }
        let results = self.db.atomic_batch(&statements).await?;
        let changed = ordered
            .into_iter()
            .zip(results.iter().step_by(4).map(|r| r.rows_affected() > 0))
            .collect::<std::collections::HashMap<_, _>>();
        Ok(ids
            .iter()
            .map(|id| changed.get(id).copied().unwrap_or(false))
            .collect())
    }
}
