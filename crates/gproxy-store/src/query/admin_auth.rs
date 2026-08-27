use sea_query::{Alias, Cond, Expr, ExprTrait, JoinType, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{insert, value};
use crate::records::UserSessionInput;

pub(crate) fn set_admin_password(
    username: &str,
    password_hash: &str,
) -> Result<Statement, StoreError> {
    let mut update = Query::update();
    update
        .table(Alias::new("users"))
        .value(Alias::new("password_hash"), password_hash.to_owned())
        .and_where(Expr::col(Alias::new("name")).eq(username))
        .and_where(Expr::col(Alias::new("is_admin")).eq(true));
    Statement::query(&update)
}

pub(crate) fn admin_by_username(username: &str) -> Result<Statement, StoreError> {
    let mut query = admin_select();
    query
        .and_where(Expr::col(Alias::new("name")).eq(username))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn insert_user_session(input: &UserSessionInput) -> Result<Statement, StoreError> {
    insert(
        "user_sessions",
        &["token_digest", "user_id", "created_at", "expires_at"],
        vec![
            value(input.token_digest.clone()),
            value(input.user_id),
            value(input.created_at),
            value(input.expires_at),
        ],
    )
}

pub(crate) fn admin_for_session(token_digest: &[u8], now: i64) -> Result<Statement, StoreError> {
    let users = Alias::new("users");
    let sessions = Alias::new("user_sessions");
    let mut query = Query::select();
    query
        .columns([
            (users.clone(), Alias::new("id")),
            (users.clone(), Alias::new("name")),
            (users.clone(), Alias::new("password_hash")),
            (users.clone(), Alias::new("enabled")),
        ])
        .from(sessions.clone())
        .join(
            JoinType::InnerJoin,
            users.clone(),
            Expr::col((sessions.clone(), Alias::new("user_id")))
                .equals((users.clone(), Alias::new("id"))),
        )
        .and_where(
            Expr::col((sessions.clone(), Alias::new("token_digest"))).eq(token_digest.to_vec()),
        )
        .and_where(Expr::col((sessions, Alias::new("expires_at"))).gt(now))
        .and_where(Expr::col((users.clone(), Alias::new("enabled"))).eq(true))
        .and_where(Expr::col((users, Alias::new("is_admin"))).eq(true))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn admin_for_api_key(digest: &[u8], now: i64) -> Result<Statement, StoreError> {
    let users = Alias::new("users");
    let keys = Alias::new("user_keys");
    let mut query = Query::select();
    query
        .columns([
            (users.clone(), Alias::new("id")),
            (users.clone(), Alias::new("name")),
            (users.clone(), Alias::new("password_hash")),
            (users.clone(), Alias::new("enabled")),
        ])
        .from(keys.clone())
        .join(
            JoinType::InnerJoin,
            users.clone(),
            Expr::col((keys.clone(), Alias::new("user_id")))
                .equals((users.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((keys, Alias::new("digest"))).eq(digest.to_vec()))
        .and_where(Expr::col((users.clone(), Alias::new("enabled"))).eq(true))
        .and_where(Expr::col((users, Alias::new("is_admin"))).eq(true))
        .and_where(Expr::col((Alias::new("user_keys"), Alias::new("enabled"))).eq(true))
        .cond_where(
            Cond::any()
                .add(Expr::col((Alias::new("user_keys"), Alias::new("expires_at"))).is_null())
                .add(Expr::col((Alias::new("user_keys"), Alias::new("expires_at"))).gt(now)),
        )
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn delete_user_session(token_digest: &[u8]) -> Result<Statement, StoreError> {
    let mut query = Query::delete();
    query
        .from_table(Alias::new("user_sessions"))
        .and_where(Expr::col(Alias::new("token_digest")).eq(token_digest.to_vec()));
    Statement::query(&query)
}

fn admin_select() -> sea_query::SelectStatement {
    let mut query = Query::select();
    query
        .columns(
            ["id", "name", "password_hash", "enabled"]
                .into_iter()
                .map(Alias::new),
        )
        .from(Alias::new("users"))
        .and_where(Expr::col(Alias::new("is_admin")).eq(true))
        .and_where(Expr::col(Alias::new("password_hash")).is_not_null());
    query.to_owned()
}
