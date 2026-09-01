use sea_query::{Alias, Cond, Expr, ExprTrait, JoinType, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{insert, value};
use crate::records::{OAuthCodeInput, OAuthDeviceInput, OAuthGrantInput, OAuthTokenInput};

pub(crate) fn insert_grant(input: &OAuthGrantInput) -> Result<Statement, StoreError> {
    insert(
        "oauth_grants",
        &[
            "user_id",
            "user_key_id",
            "provider_id",
            "client_id",
            "scopes",
            "chatgpt_user_id",
            "chatgpt_account_id",
            "created_at",
        ],
        vec![
            value(input.user_id),
            value(input.user_key_id),
            value(input.provider_id),
            value(input.client_id.clone()),
            value(input.scopes.clone()),
            value(input.chatgpt_user_id.clone()),
            value(input.chatgpt_account_id.clone()),
            value(input.created_at),
        ],
    )
}

pub(crate) fn insert_code(input: &OAuthCodeInput) -> Result<Statement, StoreError> {
    insert(
        "oauth_codes",
        &[
            "digest",
            "grant_id",
            "redirect_uri",
            "code_challenge",
            "created_at",
            "expires_at",
        ],
        vec![
            value(input.digest.clone()),
            value(input.grant_id),
            value(input.redirect_uri.clone()),
            value(input.code_challenge.clone()),
            value(input.created_at),
            value(input.expires_at),
        ],
    )
}

pub(crate) fn insert_token(input: &OAuthTokenInput) -> Result<Statement, StoreError> {
    insert(
        "oauth_tokens",
        &["digest", "grant_id", "kind", "created_at", "expires_at"],
        vec![
            value(input.digest.clone()),
            value(input.grant_id),
            value(input.kind.clone()),
            value(input.created_at),
            value(input.expires_at),
        ],
    )
}

pub(crate) fn code(digest: &[u8]) -> Result<Statement, StoreError> {
    joined(
        "oauth_codes",
        "code",
        digest,
        &[
            "redirect_uri",
            "code_challenge",
            "expires_at",
            "consumed_at",
        ],
    )
}

pub(crate) fn token(digest: &[u8]) -> Result<Statement, StoreError> {
    joined(
        "oauth_tokens",
        "token",
        digest,
        &["kind", "expires_at", "consumed_at", "revoked_at"],
    )
}

fn joined(
    table: &str,
    prefix: &str,
    digest: &[u8],
    fields: &[&str],
) -> Result<Statement, StoreError> {
    let item = Alias::new(table);
    let grants = Alias::new("oauth_grants");
    let mut query = Query::select();
    query
        .expr_as(
            Expr::col((item.clone(), Alias::new("id"))),
            Alias::new(format!("{prefix}_id")),
        )
        .columns(
            fields
                .iter()
                .map(|field| (item.clone(), Alias::new(*field))),
        )
        .columns(
            [
                "id",
                "user_id",
                "user_key_id",
                "provider_id",
                "client_id",
                "scopes",
                "chatgpt_user_id",
                "chatgpt_account_id",
                "revoked_at",
            ]
            .into_iter()
            .map(|field| (grants.clone(), Alias::new(field))),
        )
        .from(item.clone())
        .join(
            JoinType::InnerJoin,
            grants.clone(),
            Expr::col((item.clone(), Alias::new("grant_id"))).equals((grants, Alias::new("id"))),
        )
        .and_where(Expr::col((item, Alias::new("digest"))).eq(digest.to_vec()))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn consume_code(id: i64, now: i64) -> Result<Statement, StoreError> {
    consume("oauth_codes", id, now)
}

pub(crate) fn consume_token(id: i64, now: i64) -> Result<Statement, StoreError> {
    consume("oauth_tokens", id, now)
}

fn consume(table: &str, id: i64, now: i64) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new(table))
        .value(Alias::new("consumed_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("consumed_at")).is_null())
        .and_where(Expr::col(Alias::new("expires_at")).gt(now));
    Statement::query(&query)
}

pub(crate) fn revoke_grant(id: i64, now: i64) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("oauth_grants"))
        .value(Alias::new("revoked_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("revoked_at")).is_null());
    Statement::query(&query)
}

pub(crate) fn revoke_tokens(grant_id: i64, now: i64) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("oauth_tokens"))
        .value(Alias::new("revoked_at"), now)
        .and_where(Expr::col(Alias::new("grant_id")).eq(grant_id))
        .and_where(Expr::col(Alias::new("revoked_at")).is_null());
    Statement::query(&query)
}

pub(crate) fn disable_user_key(id: i64) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("user_keys"))
        .value(Alias::new("enabled"), false)
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("enabled")).eq(true));
    Statement::query(&query)
}

pub(crate) fn access_identity(digest: &[u8], now: i64) -> Result<Statement, StoreError> {
    let tokens = Alias::new("oauth_tokens");
    let grants = Alias::new("oauth_grants");
    let users = Alias::new("users");
    let keys = Alias::new("user_keys");
    let mut query = Query::select();
    query
        .columns([
            (grants.clone(), Alias::new("user_id")),
            (grants.clone(), Alias::new("user_key_id")),
            (users.clone(), Alias::new("organization_id")),
            (users.clone(), Alias::new("team_id")),
            (tokens.clone(), Alias::new("expires_at")),
        ])
        .from(tokens.clone())
        .join(
            JoinType::InnerJoin,
            grants.clone(),
            Expr::col((tokens.clone(), Alias::new("grant_id")))
                .equals((grants.clone(), Alias::new("id"))),
        )
        .join(
            JoinType::InnerJoin,
            users.clone(),
            Expr::col((grants.clone(), Alias::new("user_id")))
                .equals((users.clone(), Alias::new("id"))),
        )
        .join(
            JoinType::InnerJoin,
            keys.clone(),
            Expr::col((grants.clone(), Alias::new("user_key_id")))
                .equals((keys.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((tokens.clone(), Alias::new("digest"))).eq(digest.to_vec()))
        .and_where(Expr::col((tokens.clone(), Alias::new("kind"))).eq("access"))
        .and_where(Expr::col((tokens.clone(), Alias::new("expires_at"))).gt(now))
        .and_where(Expr::col((tokens.clone(), Alias::new("revoked_at"))).is_null())
        .and_where(Expr::col((grants, Alias::new("revoked_at"))).is_null())
        .and_where(Expr::col((users, Alias::new("enabled"))).eq(true))
        .and_where(Expr::col((keys.clone(), Alias::new("enabled"))).eq(true))
        .cond_where(
            Cond::any()
                .add(Expr::col((keys.clone(), Alias::new("expires_at"))).is_null())
                .add(Expr::col((keys, Alias::new("expires_at"))).gt(now)),
        )
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn insert_device(input: &OAuthDeviceInput) -> Result<Statement, StoreError> {
    insert(
        "oauth_devices",
        &[
            "device_digest",
            "user_code",
            "client_id",
            "provider_id",
            "created_at",
            "expires_at",
        ],
        vec![
            value(input.device_digest.clone()),
            value(input.user_code.clone()),
            value(input.client_id.clone()),
            value(input.provider_id),
            value(input.created_at),
            value(input.expires_at),
        ],
    )
}

pub(crate) fn device_by_digest(digest: &[u8]) -> Result<Statement, StoreError> {
    device(Expr::col(Alias::new("device_digest")).eq(digest.to_vec()))
}

pub(crate) fn device_by_code(code: &str) -> Result<Statement, StoreError> {
    device(Expr::col(Alias::new("user_code")).eq(code))
}

fn device(condition: sea_query::SimpleExpr) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(
            [
                "id",
                "user_code",
                "client_id",
                "provider_id",
                "expires_at",
                "grant_id",
                "approved_at",
                "consumed_at",
                "ciphertext",
                "wrapped_key",
                "payload_nonce",
                "key_nonce",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .from(Alias::new("oauth_devices"))
        .and_where(condition)
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn approve_device(
    id: i64,
    grant_id: i64,
    envelope: &crate::records::CredentialEnvelope,
    now: i64,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("oauth_devices"))
        .values([
            (Alias::new("grant_id"), value(grant_id)),
            (Alias::new("approved_at"), value(now)),
            (Alias::new("ciphertext"), value(envelope.ciphertext.clone())),
            (
                Alias::new("wrapped_key"),
                value(envelope.wrapped_key.clone()),
            ),
            (
                Alias::new("payload_nonce"),
                value(envelope.payload_nonce.clone()),
            ),
            (Alias::new("key_nonce"), value(envelope.key_nonce.clone())),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("approved_at")).is_null())
        .and_where(Expr::col(Alias::new("expires_at")).gt(now));
    Statement::query(&query)
}
