use std::collections::BTreeSet;

use tokio_rusqlite::rusqlite::{Connection, Result};

use super::super::model::{Legacy, PriceRate, PriceRule, SourceData};
use super::{decimal, optional_json};

pub(super) fn read(connection: &Connection, data: &mut SourceData) -> Result<()> {
    data.price_rules = price_rules(connection)?;
    data.price_rates = price_rates(connection)?;
    backfill_legacy_rates(data);
    Ok(())
}

fn price_rules(connection: &Connection) -> Result<Vec<Legacy<PriceRule>>> {
    let mut query = connection.prepare(
        "SELECT id,provider_id,match_type,model_match,input_price,output_price,cache_read_price,cache_creation_5m_price,cache_creation_30m_price,cache_creation_1h_price,image_output_price,pricing_tiers_json,enabled FROM price_rules ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: PriceRule {
                    provider_id: row.get(1)?,
                    match_type: row.get(2)?,
                    model_match: row.get(3)?,
                    legacy_prices: [
                        decimal(row, 4)?,
                        decimal(row, 5)?,
                        decimal(row, 6)?,
                        decimal(row, 7)?,
                        decimal(row, 8)?,
                        decimal(row, 9)?,
                        decimal(row, 10)?,
                    ],
                    tiers: optional_json(row, 11)?,
                    enabled: row.get(12)?,
                },
            })
        })?
        .collect()
}

fn price_rates(connection: &Connection) -> Result<Vec<Legacy<PriceRate>>> {
    let mut query = connection.prepare(
        "SELECT id,price_rule_id,metric,unit_size,price_usd,conditions_json,sort_order FROM price_rule_rates ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: PriceRate {
                    rule_id: row.get(1)?,
                    metric: row.get(2)?,
                    unit_size: row.get(3)?,
                    price: decimal(row, 4)?,
                    conditions: optional_json(row, 5)?,
                    sort_order: row.get(6)?,
                },
            })
        })?
        .collect()
}

fn backfill_legacy_rates(data: &mut SourceData) {
    let explicit = data
        .price_rates
        .iter()
        .map(|rate| rate.value.rule_id)
        .collect::<BTreeSet<_>>();
    let mut next_id = -1;
    for rule in &data.price_rules {
        if explicit.contains(&rule.id) {
            continue;
        }
        let metrics = [
            "input_tokens",
            "output_tokens",
            "cache_read_tokens",
            "cache_creation_5m_tokens",
            "cache_creation_30m_tokens",
            "cache_creation_1h_tokens",
            "image_output_tokens",
        ];
        for (sort_order, (metric, price)) in metrics
            .into_iter()
            .zip(rule.value.legacy_prices)
            .enumerate()
        {
            data.price_rates.push(Legacy {
                id: next_id,
                value: PriceRate {
                    rule_id: rule.id,
                    metric: metric.into(),
                    unit_size: 1_000_000,
                    price,
                    conditions: None,
                    sort_order: sort_order as i64,
                },
            });
            next_id -= 1;
        }
    }
}
