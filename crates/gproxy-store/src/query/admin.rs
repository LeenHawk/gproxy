use sea_query::{Alias, Cond, Expr, ExprTrait, JoinType, Order, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{insert, json, value};
use crate::records::{AdminSessionInput, AuditEventInput};

pub(crate) fn has_admin_accounts() -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .expr(Expr::val(1))
        .from(Alias::new("admin_accounts"))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn create_first_admin(
    username: &str,
    password_hash: &str,
    created_at: i64,
) -> Result<Statement, StoreError> {
    let mut exists = Query::select();
    exists
        .expr(Expr::val(1))
        .from(Alias::new("admin_accounts"))
        .limit(1);
    let mut values = Query::select();
    values
        .exprs([
            value(username.to_owned()),
            value(password_hash.to_owned()),
            value(true),
            value(created_at),
        ])
        .cond_where(Cond::all().not().add(Expr::exists(exists.to_owned())));
    let mut query = Query::insert();
    query
        .into_table(Alias::new("admin_accounts"))
        .columns(
            ["username", "password_hash", "enabled", "created_at"]
                .into_iter()
                .map(Alias::new),
        )
        .select_from(values.to_owned())
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Statement::query(&query)
}

pub(crate) fn admin_by_username(username: &str) -> Result<Statement, StoreError> {
    let mut query = admin_select();
    query
        .and_where(Expr::col(Alias::new("username")).eq(username))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn insert_admin_session(input: &AdminSessionInput) -> Result<Statement, StoreError> {
    insert(
        "admin_sessions",
        &["token_digest", "admin_id", "created_at", "expires_at"],
        vec![
            value(input.token_digest.clone()),
            value(input.admin_id),
            value(input.created_at),
            value(input.expires_at),
        ],
    )
}

pub(crate) fn admin_for_session(token_digest: &[u8], now: i64) -> Result<Statement, StoreError> {
    let accounts = Alias::new("admin_accounts");
    let sessions = Alias::new("admin_sessions");
    let mut query = Query::select();
    query
        .columns([
            (accounts.clone(), Alias::new("id")),
            (accounts.clone(), Alias::new("username")),
            (accounts.clone(), Alias::new("password_hash")),
            (accounts.clone(), Alias::new("enabled")),
            (accounts.clone(), Alias::new("created_at")),
        ])
        .from(sessions.clone())
        .join(
            JoinType::InnerJoin,
            accounts.clone(),
            Expr::col((sessions.clone(), Alias::new("admin_id")))
                .equals((accounts.clone(), Alias::new("id"))),
        )
        .and_where(
            Expr::col((sessions.clone(), Alias::new("token_digest"))).eq(token_digest.to_vec()),
        )
        .and_where(Expr::col((sessions, Alias::new("expires_at"))).gt(now))
        .and_where(Expr::col((accounts, Alias::new("enabled"))).eq(true))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn delete_admin_session(token_digest: &[u8]) -> Result<Statement, StoreError> {
    let mut query = Query::delete();
    query
        .from_table(Alias::new("admin_sessions"))
        .and_where(Expr::col(Alias::new("token_digest")).eq(token_digest.to_vec()));
    Statement::query(&query)
}

pub(crate) fn insert_audit_event(input: &AuditEventInput) -> Result<Statement, StoreError> {
    insert(
        "admin_audit_events",
        &[
            "actor_admin_id",
            "action",
            "target_kind",
            "target_id",
            "at",
            "details_json",
        ],
        vec![
            value(input.actor_admin_id),
            value(input.action.clone()),
            value(input.target_kind.clone()),
            value(input.target_id),
            value(input.at),
            value(
                input
                    .details
                    .as_ref()
                    .map(|details| json(details, "audit details"))
                    .transpose()?,
            ),
        ],
    )
}

pub(crate) fn select_audit_events(limit: u64) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(
            [
                "id",
                "actor_admin_id",
                "action",
                "target_kind",
                "target_id",
                "at",
                "details_json",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .from(Alias::new("admin_audit_events"))
        .order_by(Alias::new("id"), Order::Desc)
        .limit(limit);
    Statement::query(&query)
}

pub(crate) fn select_user_key_secret(id: i64) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(
            [
                "id",
                "ciphertext",
                "wrapped_key",
                "payload_nonce",
                "key_nonce",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .from(Alias::new("user_keys"))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .limit(1);
    Statement::query(&query)
}

fn admin_select() -> sea_query::SelectStatement {
    let mut query = Query::select();
    query
        .columns(
            ["id", "username", "password_hash", "enabled", "created_at"]
                .into_iter()
                .map(Alias::new),
        )
        .from(Alias::new("admin_accounts"));
    query.to_owned()
}
