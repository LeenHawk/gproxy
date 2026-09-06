use gproxy_core::channel_api::{CODEX_OAUTH_CLIENT_ID, PI_OAUTH_CLIENT_ID};
use sea_query::{Alias, Expr, ExprTrait, Func, Query, SimpleExpr};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::oauth_clients;
use crate::records::OAuthClientInput;

pub(crate) fn statements() -> Result<Vec<Statement>, StoreError> {
    let clients = [
        OAuthClientInput {
            client_id: CODEX_OAUTH_CLIENT_ID.into(),
            name: "Codex CLI".into(),
            enabled: true,
            redirect_uris: vec![
                "http://localhost:1455/auth/callback".into(),
                "http://localhost:1457/auth/callback".into(),
            ],
        },
        OAuthClientInput {
            client_id: PI_OAUTH_CLIENT_ID.into(),
            name: "GPROXY for Pi".into(),
            enabled: false,
            redirect_uris: vec!["http://127.0.0.1/oauth/callback".into()],
        },
    ];
    let mut statements = clients
        .iter()
        .map(oauth_clients::create)
        .collect::<Result<Vec<_>, _>>()?;
    let count = token_aggregate("refresh", Func::count(Expr::col(Alias::new("id"))).into());
    let mut backfill = Query::update();
    backfill.table(Alias::new("oauth_grants")).values([
        (
            Alias::new("logged_in_at"),
            token_aggregate(
                "access",
                Func::min(Expr::col(Alias::new("created_at"))).into(),
            ),
        ),
        (
            Alias::new("refresh_expires_at"),
            token_aggregate(
                "refresh",
                Func::max(Expr::col(Alias::new("expires_at"))).into(),
            ),
        ),
        (
            Alias::new("refresh_count"),
            Expr::case(count.clone().gt(0), count.clone().sub(1))
                .finally(Option::<i64>::None)
                .into(),
        ),
        (
            Alias::new("last_refreshed_at"),
            Expr::case(
                count.gt(1),
                token_aggregate(
                    "refresh",
                    Func::max(Expr::col(Alias::new("created_at"))).into(),
                ),
            )
            .finally(Option::<i64>::None)
            .into(),
        ),
    ]);
    statements.push(Statement::query(&backfill)?);
    Ok(statements)
}

fn token_aggregate(kind: &str, expression: SimpleExpr) -> SimpleExpr {
    Query::select()
        .expr(expression)
        .from(Alias::new("oauth_tokens"))
        .and_where(
            Expr::col((Alias::new("oauth_tokens"), Alias::new("grant_id")))
                .equals((Alias::new("oauth_grants"), Alias::new("id"))),
        )
        .and_where(Expr::col(Alias::new("kind")).eq(kind))
        .to_owned()
        .into()
}
