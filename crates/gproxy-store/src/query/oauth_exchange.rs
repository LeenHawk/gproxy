use sea_query::{Alias, Expr, ExprTrait, Query, SelectStatement};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::value;
use crate::records::{OAuthExchangeSource, OAuthTokenInput};

pub(crate) fn exchange(
    source: OAuthExchangeSource,
    client_id: &str,
    access: &OAuthTokenInput,
    refresh: &OAuthTokenInput,
) -> Result<Vec<Statement>, StoreError> {
    let (table, id) = match source {
        OAuthExchangeSource::Code(id) => ("oauth_codes", id),
        OAuthExchangeSource::Refresh(id) => ("oauth_tokens", id),
    };
    let mut grant_lock = Query::update();
    grant_lock
        .table(Alias::new("oauth_grants"))
        .value(
            Alias::new("revoked_at"),
            Expr::col(Alias::new("revoked_at")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(access.grant_id));
    let now = access.created_at;
    let mut consume = Query::update();
    consume
        .table(Alias::new(table))
        .values([
            (Alias::new("consumed_at"), value(now)),
            (Alias::new("consumed_by"), value(refresh.digest.clone())),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("grant_id")).eq(access.grant_id))
        .and_where(Expr::col(Alias::new("consumed_at")).is_null())
        .and_where(Expr::col(Alias::new("expires_at")).gt(now))
        .and_where(
            Expr::col(Alias::new("grant_id"))
                .in_subquery(super::oauth::live_grants(client_id, now)),
        );
    if matches!(source, OAuthExchangeSource::Refresh(_)) {
        consume
            .and_where(Expr::col(Alias::new("kind")).eq("refresh"))
            .and_where(Expr::col(Alias::new("revoked_at")).is_null());
    }
    let mut record = Query::update();
    record
        .table(Alias::new("oauth_grants"))
        .value(Alias::new("refresh_expires_at"), refresh.expires_at)
        .and_where(Expr::col(Alias::new("id")).in_subquery(receipt(table, id, refresh)));
    match source {
        OAuthExchangeSource::Code(_) => {
            record
                .value(Alias::new("logged_in_at"), now)
                .value(Alias::new("refresh_count"), 0);
        }
        OAuthExchangeSource::Refresh(_) => {
            record.value(Alias::new("last_refreshed_at"), now).value(
                Alias::new("refresh_count"),
                Expr::col(Alias::new("refresh_count")).add(1),
            );
        }
    }
    Ok(vec![
        super::oauth_clients::lock(client_id)?,
        Statement::query(&grant_lock)?,
        Statement::query(&consume)?,
        issued(table, id, access, refresh)?,
        issued(table, id, refresh, refresh)?,
        Statement::query(&record)?,
    ])
}

fn receipt(table: &str, id: i64, refresh: &OAuthTokenInput) -> SelectStatement {
    Query::select()
        .column(Alias::new("grant_id"))
        .from(Alias::new(table))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("consumed_by")).eq(refresh.digest.clone()))
        .to_owned()
}

fn issued(
    table: &str,
    id: i64,
    token: &OAuthTokenInput,
    refresh: &OAuthTokenInput,
) -> Result<Statement, StoreError> {
    let mut select = receipt(table, id, refresh);
    select.clear_selects();
    select.exprs([
        value(token.digest.clone()),
        value(token.grant_id),
        value(token.kind.clone()),
        value(token.created_at),
        value(token.expires_at),
    ]);
    let mut insert = Query::insert();
    insert
        .into_table(Alias::new("oauth_tokens"))
        .columns(["digest", "grant_id", "kind", "created_at", "expires_at"].map(Alias::new))
        .select_from(select)
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Statement::query(&insert)
}
