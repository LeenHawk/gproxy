use sea_query::{Alias, Expr, ExprTrait, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{insert, value};
use crate::records::OAuthClientInput;

pub(crate) fn lock(client_id: &str) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("oauth_clients"))
        .value(Alias::new("enabled"), Expr::col(Alias::new("enabled")))
        .and_where(Expr::col(Alias::new("client_id")).eq(client_id));
    Statement::query(&query)
}

pub(crate) fn enabled(client_id: &str) -> sea_query::SelectStatement {
    Query::select()
        .from(Alias::new("oauth_clients"))
        .and_where(Expr::col(Alias::new("client_id")).eq(client_id))
        .and_where(Expr::col(Alias::new("enabled")).eq(1))
        .and_where(Expr::col(Alias::new("deleted_at")).is_null())
        .to_owned()
}

pub(crate) fn create(input: &OAuthClientInput) -> Result<Statement, StoreError> {
    insert(
        "oauth_clients",
        &["client_id", "name", "redirect_uris", "enabled"],
        vec![
            value(input.client_id.clone()),
            value(input.name.clone()),
            value(serde_json::to_string(&input.redirect_uris).expect("redirects serialize")),
            value(i64::from(input.enabled)),
        ],
    )
}

pub(crate) fn list(client_id: Option<&str>) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(
            [
                "id",
                "client_id",
                "name",
                "redirect_uris",
                "enabled",
                "deleted_at",
            ]
            .map(Alias::new),
        )
        .from(Alias::new("oauth_clients"));
    if let Some(client_id) = client_id {
        query.and_where(Expr::col(Alias::new("client_id")).eq(client_id));
    } else {
        query.and_where(Expr::col(Alias::new("deleted_at")).is_null());
    }
    query.order_by(Alias::new("id"), sea_query::Order::Asc);
    Statement::query(&query)
}

pub(crate) fn update(
    id: i64,
    input: &OAuthClientInput,
    deleted_at: Option<i64>,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("oauth_clients"))
        .values([
            (Alias::new("name"), value(input.name.clone())),
            (
                Alias::new("redirect_uris"),
                value(serde_json::to_string(&input.redirect_uris).expect("redirects serialize")),
            ),
            (
                Alias::new("enabled"),
                value(i64::from(input.enabled && deleted_at.is_none())),
            ),
            (Alias::new("deleted_at"), value(deleted_at)),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("client_id")).eq(input.client_id.clone()))
        .and_where(Expr::col(Alias::new("deleted_at")).is_null());
    Statement::query(&query)
}

pub(crate) fn revoke_sessions(client_id: &str, now: i64) -> Result<Vec<Statement>, StoreError> {
    let mut grants = Query::select();
    grants
        .column(Alias::new("id"))
        .from(Alias::new("oauth_grants"))
        .and_where(Expr::col(Alias::new("client_id")).eq(client_id));
    let mut keys = Query::select();
    keys.column(Alias::new("user_key_id"))
        .from(Alias::new("oauth_grants"))
        .and_where(Expr::col(Alias::new("client_id")).eq(client_id));
    let mut revoke = Query::update();
    revoke
        .table(Alias::new("oauth_grants"))
        .value(Alias::new("revoked_at"), now)
        .and_where(Expr::col(Alias::new("client_id")).eq(client_id))
        .and_where(Expr::col(Alias::new("revoked_at")).is_null());
    let mut tokens = Query::update();
    tokens
        .table(Alias::new("oauth_tokens"))
        .value(Alias::new("revoked_at"), now)
        .and_where(Expr::col(Alias::new("grant_id")).in_subquery(grants))
        .and_where(Expr::col(Alias::new("revoked_at")).is_null());
    let mut disable = Query::update();
    disable
        .table(Alias::new("user_keys"))
        .value(Alias::new("enabled"), 0)
        .and_where(Expr::col(Alias::new("id")).in_subquery(keys));
    let mut devices = Query::update();
    devices
        .table(Alias::new("oauth_devices"))
        .value(Alias::new("denied_at"), now)
        .and_where(Expr::col(Alias::new("client_id")).eq(client_id))
        .and_where(Expr::col(Alias::new("denied_at")).is_null());
    Ok(vec![
        Statement::query(&revoke)?,
        Statement::query(&tokens)?,
        Statement::query(&disable)?,
        Statement::query(&devices)?,
    ])
}
