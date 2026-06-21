//! Route-member ops for the libSQL edge backend.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_bool, col_i64, col_str};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{RouteMember, RouteMemberInput};

const COLS: &str = "id, route_id, provider_id, upstream_model_id, weight, tier, enabled, \
     created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<RouteMember> {
    Ok(RouteMember {
        id: col_i64(row, 0)?,
        route_id: col_i64(row, 1)?,
        provider_id: col_i64(row, 2)?,
        upstream_model_id: col_str(row, 3)?,
        weight: col_i64(row, 4)?,
        tier: col_i64(row, 5)?,
        enabled: col_bool(row, 6)?,
        created_at: col_i64(row, 7)?,
        updated_at: col_i64(row, 8)?,
    })
}

async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<RouteMember>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM route_members WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient, route_id: i64) -> anyhow::Result<Vec<RouteMember>> {
    query(
        client,
        &format!("SELECT {COLS} FROM route_members WHERE route_id = ?"),
        &[arg_integer(route_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn upsert(client: &LibsqlClient, input: RouteMemberInput) -> anyhow::Result<RouteMember> {
    let now = now_secs();

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            exec(
                client,
                "UPDATE route_members SET route_id=?, provider_id=?, upstream_model_id=?, \
                 weight=?, tier=?, enabled=?, updated_at=? WHERE id=?",
                &[
                    arg_integer(input.route_id),
                    arg_integer(input.provider_id),
                    arg_text(&input.upstream_model_id),
                    arg_integer(input.weight),
                    arg_integer(input.tier),
                    arg_bool(input.enabled),
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
                    "INSERT INTO route_members \
                     (id, route_id, provider_id, upstream_model_id, weight, tier, enabled, \
                      created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_integer(input.route_id),
                        arg_integer(input.provider_id),
                        arg_text(&input.upstream_model_id),
                        arg_integer(input.weight),
                        arg_integer(input.tier),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql insert route_member: {e}"))?;
            match maybe_id {
                Some(id) => id,
                None => last_rowid(&qr)?,
            }
        }
    };

    get(client, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("route_member vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM route_members WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete_by_route(client: &LibsqlClient, route_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM route_members WHERE route_id = ?",
        &[arg_integer(route_id)],
    )
    .await?;
    Ok(())
}
