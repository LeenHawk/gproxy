//! Rate-limit ops for the libSQL edge backend.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_i64, col_opt_i64, col_str};
use crate::store::persistence::libsql::util::{
    arg_opt_i64, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{RateLimit, RateLimitInput, Scope};

const COLS: &str = "id, scope, scope_id, route_pattern, rpm, rpd, total_tokens, \
     created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<RateLimit> {
    Ok(RateLimit {
        id: col_i64(row, 0)?,
        scope: Scope::parse(&col_str(row, 1)?)?,
        scope_id: col_i64(row, 2)?,
        route_pattern: col_str(row, 3)?,
        rpm: col_opt_i64(row, 4)?,
        rpd: col_opt_i64(row, 5)?,
        total_tokens: col_opt_i64(row, 6)?,
        created_at: col_i64(row, 7)?,
        updated_at: col_i64(row, 8)?,
    })
}

async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<RateLimit>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM rate_limits WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(
    client: &LibsqlClient,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<Vec<RateLimit>> {
    query(
        client,
        &format!("SELECT {COLS} FROM rate_limits WHERE scope = ? AND scope_id = ?"),
        &[arg_text(scope.as_str()), arg_integer(scope_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn upsert(client: &LibsqlClient, input: RateLimitInput) -> anyhow::Result<RateLimit> {
    let now = now_secs();

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            exec(
                client,
                "UPDATE rate_limits SET scope=?, scope_id=?, route_pattern=?, rpm=?, rpd=?, \
                 total_tokens=?, updated_at=? WHERE id=?",
                &[
                    arg_text(input.scope.as_str()),
                    arg_integer(input.scope_id),
                    arg_text(&input.route_pattern),
                    arg_opt_i64(input.rpm),
                    arg_opt_i64(input.rpd),
                    arg_opt_i64(input.total_tokens),
                    arg_integer(now),
                    arg_integer(id),
                ],
            )
            .await?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO rate_limits \
                     (id, scope, scope_id, route_pattern, rpm, rpd, total_tokens, created_at, \
                      updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_text(input.scope.as_str()),
                        arg_integer(input.scope_id),
                        arg_text(&input.route_pattern),
                        arg_opt_i64(input.rpm),
                        arg_opt_i64(input.rpd),
                        arg_opt_i64(input.total_tokens),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql insert rate_limit: {e}"))?;
            match maybe_id {
                Some(id) => id,
                None => last_rowid(&qr)?,
            }
        }
    };

    get(client, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("rate_limit vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM rate_limits WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete_by_scope(
    client: &LibsqlClient,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM rate_limits WHERE scope = ? AND scope_id = ?",
        &[arg_text(scope.as_str()), arg_integer(scope_id)],
    )
    .await?;
    Ok(())
}
