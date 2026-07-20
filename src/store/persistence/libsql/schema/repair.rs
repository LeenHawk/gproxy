use std::collections::HashSet;

use crate::store::libsql::LibsqlClient;
use crate::store::persistence::libsql::row::col_str;

pub(super) async fn usage(client: &LibsqlClient) -> anyhow::Result<()> {
    let qr = client
        .execute("PRAGMA table_info(usages)", &[])
        .await
        .map_err(|e| anyhow::anyhow!("libsql inspect usages columns failed: {e}"))?;
    let cols = qr
        .rows
        .iter()
        .map(|row| col_str(row, 1))
        .collect::<anyhow::Result<HashSet<_>>>()?;
    if !cols.is_empty() && !cols.contains("cache_creation_30m_tokens") {
        client
            .execute(
                "ALTER TABLE usages ADD COLUMN cache_creation_30m_tokens INTEGER NOT NULL DEFAULT 0",
                &[],
            )
            .await
            .map_err(|e| anyhow::anyhow!("libsql repair usages add 30m cache column: {e}"))?;
    }
    Ok(())
}

pub(super) async fn price_rules(client: &LibsqlClient) -> anyhow::Result<()> {
    let cols = price_rule_columns(client).await?;
    if cols.is_empty() {
        return Ok(());
    }

    let had_rates_json = cols.contains("rates_json");
    let had_cache_write_price = cols.contains("cache_write_price");
    let had_operation = cols.contains("operation");
    let had_kind = cols.contains("kind");
    let had_priority = cols.contains("priority");
    let mut changed = false;

    for col in [
        "input_price",
        "output_price",
        "cache_read_price",
        "cache_creation_5m_price",
        "cache_creation_30m_price",
        "cache_creation_1h_price",
        "image_price",
    ] {
        if !cols.contains(col) {
            client
                .execute(
                    &format!("ALTER TABLE price_rules ADD COLUMN {col} TEXT NOT NULL DEFAULT '0'"),
                    &[],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql repair price_rules add {col}: {e}"))?;
            changed = true;
        }
    }

    if changed && had_rates_json {
        let sql = "UPDATE price_rules SET \
                input_price = COALESCE(CAST(json_extract(rates_json, '$.input_tokens') AS TEXT), CAST(json_extract(rates_json, '$.input') AS TEXT), input_price), \
                output_price = COALESCE(CAST(json_extract(rates_json, '$.output_tokens') AS TEXT), CAST(json_extract(rates_json, '$.output') AS TEXT), output_price), \
                cache_read_price = COALESCE(CAST(json_extract(rates_json, '$.cache_read_tokens') AS TEXT), CAST(json_extract(rates_json, '$.cache_read') AS TEXT), cache_read_price), \
                cache_creation_5m_price = COALESCE(CAST(json_extract(rates_json, '$.cache_write_tokens') AS TEXT), CAST(json_extract(rates_json, '$.cache_creation') AS TEXT), cache_creation_5m_price), \
                cache_creation_1h_price = COALESCE(CAST(json_extract(rates_json, '$.cache_write_tokens') AS TEXT), CAST(json_extract(rates_json, '$.cache_creation') AS TEXT), cache_creation_1h_price), \
                image_price = CASE \
                    WHEN json_type(rates_json, '$.image_count') IN ('integer', 'real', 'text') THEN CAST(json_extract(rates_json, '$.image_count') AS TEXT) \
                    WHEN json_type(rates_json, '$.image') IN ('integer', 'real', 'text') THEN CAST(json_extract(rates_json, '$.image') AS TEXT) \
                    ELSE image_price \
                END \
             WHERE rates_json IS NOT NULL AND rates_json <> '' AND json_valid(rates_json)";
        if let Err(err) = client.execute(sql, &[]).await {
            tracing::warn!(error = %err, "libsql price_rules rates_json backfill skipped");
        }
    }

    if changed && had_cache_write_price {
        let sql = "UPDATE price_rules \
                   SET cache_creation_5m_price = cache_write_price, \
                       cache_creation_1h_price = cache_write_price \
                   WHERE cache_write_price IS NOT NULL";
        if let Err(err) = client.execute(sql, &[]).await {
            tracing::warn!(error = %err, "libsql price_rules cache_write_price backfill skipped");
        }
    }

    if had_rates_json || had_cache_write_price || had_operation || had_kind || had_priority {
        rebuild_price_rules_table_without_legacy_columns(client).await?;
    }
    Ok(())
}

async fn rebuild_price_rules_table_without_legacy_columns(
    client: &LibsqlClient,
) -> anyhow::Result<()> {
    for sql in [
        "DROP TABLE IF EXISTS price_rules_repaired",
        "CREATE TABLE price_rules_repaired (\
            id INTEGER PRIMARY KEY, \
            provider_id INTEGER, \
            match_type TEXT NOT NULL, \
            model_match TEXT NOT NULL, \
            input_price TEXT NOT NULL, \
            output_price TEXT NOT NULL, \
            cache_read_price TEXT NOT NULL, \
            cache_creation_5m_price TEXT NOT NULL, \
            cache_creation_30m_price TEXT NOT NULL, \
            cache_creation_1h_price TEXT NOT NULL, \
            image_price TEXT NOT NULL, \
            enabled INTEGER NOT NULL, \
            created_at INTEGER NOT NULL, \
            updated_at INTEGER NOT NULL)",
        "INSERT INTO price_rules_repaired \
            (id, provider_id, match_type, model_match, \
             input_price, output_price, cache_read_price, cache_creation_5m_price, \
             cache_creation_30m_price, cache_creation_1h_price, image_price, enabled, created_at, updated_at) \
         SELECT \
            id, provider_id, match_type, model_match, \
            input_price, output_price, cache_read_price, cache_creation_5m_price, \
            cache_creation_30m_price, cache_creation_1h_price, image_price, enabled, created_at, updated_at \
         FROM price_rules",
        "DROP TABLE price_rules",
        "ALTER TABLE price_rules_repaired RENAME TO price_rules",
    ] {
        client
            .execute(sql, &[])
            .await
            .map_err(|e| anyhow::anyhow!("libsql repair price_rules rebuild failed: {e}"))?;
    }
    Ok(())
}

async fn price_rule_columns(client: &LibsqlClient) -> anyhow::Result<HashSet<String>> {
    let qr = client
        .execute("PRAGMA table_info(price_rules)", &[])
        .await
        .map_err(|e| anyhow::anyhow!("libsql inspect price_rules columns failed: {e}"))?;
    qr.rows
        .iter()
        .map(|row| col_str(row, 1))
        .collect::<anyhow::Result<HashSet<_>>>()
}
