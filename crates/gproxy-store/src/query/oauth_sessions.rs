use sea_query::{
    Alias, Asterisk, Cond, Expr, ExprTrait, Func, JoinType, Query, SelectStatement, SimpleExpr,
};

use crate::StoreError;
use crate::backend::Statement;

fn field(table: &str, column: &str) -> SimpleExpr {
    Expr::col((Alias::new(table), Alias::new(column)))
}

pub(crate) fn active(now: i64) -> Cond {
    Cond::all()
        .add(field("oauth_grants", "revoked_at").is_null())
        .add(field("oauth_grants", "refresh_expires_at").gt(now))
        .add(field("oauth_clients", "enabled").eq(1))
        .add(field("oauth_clients", "deleted_at").is_null())
        .add(field("users", "enabled").eq(1))
        .add(field("user_keys", "enabled").eq(1))
        .add(
            Cond::any()
                .add(field("user_keys", "expires_at").is_null())
                .add(field("user_keys", "expires_at").gt(now)),
        )
}

fn base(user_id: i64) -> SelectStatement {
    let mut query = Query::select();
    query
        .from(Alias::new("oauth_grants"))
        .join(
            JoinType::InnerJoin,
            Alias::new("oauth_clients"),
            field("oauth_grants", "client_id")
                .equals((Alias::new("oauth_clients"), Alias::new("client_id"))),
        )
        .join(
            JoinType::InnerJoin,
            Alias::new("users"),
            field("oauth_grants", "user_id").equals((Alias::new("users"), Alias::new("id"))),
        )
        .join(
            JoinType::InnerJoin,
            Alias::new("user_keys"),
            field("oauth_grants", "user_key_id")
                .equals((Alias::new("user_keys"), Alias::new("id"))),
        )
        .and_where(field("oauth_grants", "user_id").eq(user_id))
        .and_where(field("oauth_grants", "logged_in_at").is_not_null());
    query
}

pub(crate) fn summary(user_id: i64, now: i64) -> Result<Statement, StoreError> {
    let mut query = base(user_id);
    query
        .expr_as(Expr::col(Asterisk).count(), Alias::new("total_logins"))
        .expr_as(
            Func::sum(Expr::case(active(now), 1).finally(0)),
            Alias::new("active_sessions"),
        );
    Statement::query(&query)
}

pub(crate) fn list(
    user_id: i64,
    now: i64,
    active_only: bool,
    limit: u64,
    offset: u64,
) -> Result<Statement, StoreError> {
    let mut query = base(user_id);
    for column in [
        "id",
        "client_id",
        "logged_in_at",
        "last_refreshed_at",
        "refresh_count",
        "refresh_expires_at",
        "revoked_at",
    ] {
        query.expr_as(field("oauth_grants", column), Alias::new(column));
    }
    query
        .expr_as(field("oauth_clients", "name"), Alias::new("client_name"))
        .expr_as(Expr::case(active(now), 1).finally(0), Alias::new("active"));
    if active_only {
        query.cond_where(active(now));
    }
    query
        .order_by(
            (Alias::new("oauth_grants"), Alias::new("logged_in_at")),
            sea_query::Order::Desc,
        )
        .order_by(
            (Alias::new("oauth_grants"), Alias::new("id")),
            sea_query::Order::Desc,
        )
        .limit(limit)
        .offset(offset);
    Statement::query(&query)
}

pub(crate) fn owned_key(user_id: i64, id: i64) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .column(Alias::new("user_key_id"))
        .from(Alias::new("oauth_grants"))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("user_id")).eq(user_id))
        .and_where(Expr::col(Alias::new("logged_in_at")).is_not_null());
    Statement::query(&query)
}

pub(crate) fn internal_keys() -> SelectStatement {
    Query::select()
        .column(Alias::new("user_key_id"))
        .from(Alias::new("oauth_grants"))
        .to_owned()
}

pub(crate) fn internal_key(id: i64) -> Result<Statement, StoreError> {
    let mut query = internal_keys();
    query
        .and_where(Expr::col(Alias::new("user_key_id")).eq(id))
        .limit(1);
    Statement::query(&query)
}
