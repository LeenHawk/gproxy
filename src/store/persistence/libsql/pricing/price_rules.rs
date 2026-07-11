//! Pricing rule ops for the libSQL edge backend.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_bool, col_i64, col_opt_i64, col_str};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{PriceRule, PriceRuleInput};

const COLS: &str = "id, provider_id, match_type, model_match, \
     input_price, output_price, cache_read_price, cache_creation_5m_price, \
     cache_creation_30m_price, cache_creation_1h_price, image_price, \
     enabled, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<PriceRule> {
    Ok(PriceRule {
        id: col_i64(row, 0)?,
        provider_id: col_opt_i64(row, 1)?,
        match_type: col_str(row, 2)?,
        model_match: col_str(row, 3)?,
        input_price: col_str(row, 4)?.parse()?,
        output_price: col_str(row, 5)?.parse()?,
        cache_read_price: col_str(row, 6)?.parse()?,
        cache_creation_5m_price: col_str(row, 7)?.parse()?,
        cache_creation_30m_price: col_str(row, 8)?.parse()?,
        cache_creation_1h_price: col_str(row, 9)?.parse()?,
        image_price: col_str(row, 10)?.parse()?,
        enabled: col_bool(row, 11)?,
        created_at: col_i64(row, 12)?,
        updated_at: col_i64(row, 13)?,
    })
}

async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<PriceRule>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM price_rules WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient) -> anyhow::Result<Vec<PriceRule>> {
    query(client, &format!("SELECT {COLS} FROM price_rules"), &[])
        .await?
        .iter()
        .map(decode)
        .collect()
}

pub async fn upsert(client: &LibsqlClient, input: PriceRuleInput) -> anyhow::Result<PriceRule> {
    let now = now_secs();

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            exec(
                client,
                "UPDATE price_rules SET provider_id=?, match_type=?, model_match=?, \
                 input_price=?, output_price=?, cache_read_price=?, \
                 cache_creation_5m_price=?, cache_creation_30m_price=?, \
                 cache_creation_1h_price=?, image_price=?, \
                 enabled=?, updated_at=? WHERE id=?",
                &[
                    arg_opt_i64(input.provider_id),
                    arg_text(&input.match_type),
                    arg_text(&input.model_match),
                    arg_text(&input.input_price.to_string()),
                    arg_text(&input.output_price.to_string()),
                    arg_text(&input.cache_read_price.to_string()),
                    arg_text(&input.cache_creation_5m_price.to_string()),
                    arg_text(&input.cache_creation_30m_price.to_string()),
                    arg_text(&input.cache_creation_1h_price.to_string()),
                    arg_text(&input.image_price.to_string()),
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
                    "INSERT INTO price_rules \
                     (id, provider_id, match_type, model_match, \
                      input_price, output_price, cache_read_price, cache_creation_5m_price, \
                      cache_creation_30m_price, cache_creation_1h_price, image_price, \
                      enabled, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_opt_i64(input.provider_id),
                        arg_text(&input.match_type),
                        arg_text(&input.model_match),
                        arg_text(&input.input_price.to_string()),
                        arg_text(&input.output_price.to_string()),
                        arg_text(&input.cache_read_price.to_string()),
                        arg_text(&input.cache_creation_5m_price.to_string()),
                        arg_text(&input.cache_creation_30m_price.to_string()),
                        arg_text(&input.cache_creation_1h_price.to_string()),
                        arg_text(&input.image_price.to_string()),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql insert price_rule: {e}"))?;
            match maybe_id {
                Some(id) => id,
                None => last_rowid(&qr)?,
            }
        }
    };

    get(client, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("price_rule vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM price_rules WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}
