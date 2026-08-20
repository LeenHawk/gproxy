//! Pricing rule ops for the libSQL edge backend.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_bool, col_i64, col_opt_i64, col_opt_json, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use std::collections::HashMap;

use crate::store::persistence::records::{PriceRate, PriceRule, PriceRuleInput};

const COLS: &str = "id, provider_id, match_type, model_match, \
     input_price, output_price, cache_read_price, cache_creation_5m_price, \
     cache_creation_30m_price, cache_creation_1h_price, image_output_price, \
     pricing_tiers_json, enabled, created_at, updated_at";

fn decode(row: &Row, rates: Vec<PriceRate>) -> anyhow::Result<PriceRule> {
    let mut record = PriceRule {
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
        image_output_price: col_str(row, 10)?.parse()?,
        pricing_tiers_json: col_opt_json(row, 11)?,
        rates,
        enabled: col_bool(row, 12)?,
        created_at: col_i64(row, 13)?,
        updated_at: col_i64(row, 14)?,
    };
    record.apply_rate_projections();
    Ok(record)
}

async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<PriceRule>> {
    let row = query_one(
        client,
        &format!("SELECT {COLS} FROM price_rules WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?;
    let Some(row) = row.as_ref() else {
        return Ok(None);
    };
    Ok(Some(decode(
        row,
        rates_for(client, Some(id))
            .await?
            .remove(&id)
            .unwrap_or_default(),
    )?))
}

pub async fn list(client: &LibsqlClient) -> anyhow::Result<Vec<PriceRule>> {
    let mut rates = rates_for(client, None).await?;
    query(client, &format!("SELECT {COLS} FROM price_rules"), &[])
        .await?
        .iter()
        .map(|row| {
            let id = col_i64(row, 0)?;
            decode(row, rates.remove(&id).unwrap_or_default())
        })
        .collect()
}

async fn rates_for(
    client: &LibsqlClient,
    price_rule_id: Option<i64>,
) -> anyhow::Result<HashMap<i64, Vec<PriceRate>>> {
    let (sql, args) = match price_rule_id {
        Some(id) => (
            "SELECT price_rule_id, metric, unit, unit_size, price_usd, conditions_json, sort_order FROM price_rule_rates WHERE price_rule_id = ? ORDER BY sort_order, id",
            vec![arg_integer(id)],
        ),
        None => (
            "SELECT price_rule_id, metric, unit, unit_size, price_usd, conditions_json, sort_order FROM price_rule_rates ORDER BY price_rule_id, sort_order, id",
            Vec::new(),
        ),
    };
    let mut grouped: HashMap<i64, Vec<PriceRate>> = HashMap::new();
    for row in query(client, sql, &args).await? {
        grouped
            .entry(col_i64(&row, 0)?)
            .or_default()
            .push(PriceRate {
                metric: col_str(&row, 1)?,
                unit: col_str(&row, 2)?,
                unit_size: u64::try_from(col_i64(&row, 3)?).unwrap_or(1),
                price_usd: col_str(&row, 4)?.parse()?,
                conditions_json: col_opt_json(&row, 5)?,
                sort_order: col_i64(&row, 6)?,
            });
    }
    Ok(grouped)
}

pub async fn upsert(client: &LibsqlClient, input: PriceRuleInput) -> anyhow::Result<PriceRule> {
    input.validate_rates()?;
    let now = now_secs();
    let rates = input.effective_rates();

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            exec(
                client,
                "UPDATE price_rules SET provider_id=?, match_type=?, model_match=?, \
                 input_price=?, output_price=?, cache_read_price=?, \
                 cache_creation_5m_price=?, cache_creation_30m_price=?, \
                 cache_creation_1h_price=?, image_output_price=?, \
                 pricing_tiers_json=?, enabled=?, updated_at=? WHERE id=?",
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
                    arg_text(&input.image_output_price.to_string()),
                    arg_opt_text(
                        input
                            .pricing_tiers_json
                            .as_ref()
                            .map(serde_json::to_string)
                            .transpose()?
                            .as_deref(),
                    ),
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
                      cache_creation_30m_price, cache_creation_1h_price, image_output_price, \
                      pricing_tiers_json, enabled, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
                        arg_text(&input.image_output_price.to_string()),
                        arg_opt_text(
                            input
                                .pricing_tiers_json
                                .as_ref()
                                .map(serde_json::to_string)
                                .transpose()?
                                .as_deref(),
                        ),
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

    exec(
        client,
        "DELETE FROM price_rule_rates WHERE price_rule_id = ?",
        &[arg_integer(id)],
    )
    .await?;
    for rate in rates {
        exec(
            client,
            "INSERT INTO price_rule_rates (price_rule_id, metric, unit, unit_size, price_usd, conditions_json, sort_order) VALUES (?, ?, ?, ?, ?, ?, ?)",
            &[
                arg_integer(id),
                arg_text(&rate.metric),
                arg_text(&rate.unit),
                arg_integer(i64::try_from(rate.unit_size).unwrap_or(i64::MAX)),
                arg_text(&rate.price_usd.to_string()),
                arg_opt_text(
                    rate
                        .conditions_json
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?
                        .as_deref(),
                ),
                arg_integer(rate.sort_order),
            ],
        )
        .await?;
    }

    get(client, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("price_rule vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    exec(
        client,
        "DELETE FROM price_rule_rates WHERE price_rule_id = ?",
        &[arg_integer(id)],
    )
    .await?;
    let n = exec(
        client,
        "DELETE FROM price_rules WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}
