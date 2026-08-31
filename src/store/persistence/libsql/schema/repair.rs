use std::collections::HashSet;

use crate::store::libsql::LibsqlClient;
use crate::store::persistence::libsql::row::col_str;

pub(super) async fn instance_settings(client: &LibsqlClient) -> anyhow::Result<()> {
    let qr = client
        .execute("PRAGMA table_info(instance_settings)", &[])
        .await
        .map_err(|e| anyhow::anyhow!("libsql inspect instance_settings columns failed: {e}"))?;
    let cols = qr
        .rows
        .iter()
        .map(|row| col_str(row, 1))
        .collect::<anyhow::Result<HashSet<_>>>()?;
    if !cols.is_empty() && !cols.contains("max_database_size_mb") {
        client
            .execute(
                "ALTER TABLE instance_settings ADD COLUMN max_database_size_mb INTEGER",
                &[],
            )
            .await
            .map_err(|e| anyhow::anyhow!("libsql repair instance_settings size limit: {e}"))?;
    }
    if !cols.is_empty() && !cols.contains("enable_auto_update_check") {
        client
            .execute(
                "ALTER TABLE instance_settings ADD COLUMN enable_auto_update_check INTEGER NOT NULL DEFAULT 0",
                &[],
            )
            .await
            .map_err(|e| anyhow::anyhow!("libsql repair automatic update check setting: {e}"))?;
    }
    if !cols.is_empty() && !cols.contains("file_upload_max_in_flight") {
        client.execute("ALTER TABLE instance_settings ADD COLUMN file_upload_max_in_flight INTEGER NOT NULL DEFAULT 0", &[]).await
            .map_err(|e| anyhow::anyhow!("libsql repair file upload concurrency setting: {e}"))?;
    }
    if !cols.is_empty() && !cols.contains("request_blacklist") {
        client
            .execute(
                "ALTER TABLE instance_settings ADD COLUMN request_blacklist TEXT",
                &[],
            )
            .await
            .map_err(|e| anyhow::anyhow!("libsql repair request blacklist setting: {e}"))?;
    }
    Ok(())
}

pub(super) async fn provider_models(client: &LibsqlClient) -> anyhow::Result<()> {
    let qr = client
        .execute("PRAGMA table_info(provider_models)", &[])
        .await
        .map_err(|e| anyhow::anyhow!("libsql inspect provider_models columns failed: {e}"))?;
    let cols = qr
        .rows
        .iter()
        .map(|row| col_str(row, 1))
        .collect::<anyhow::Result<HashSet<_>>>()?;
    if cols.is_empty() {
        return Ok(());
    }
    for column in ["context_window", "max_output_tokens"] {
        if !cols.contains(column) {
            client
                .execute(
                    &format!("ALTER TABLE provider_models ADD COLUMN {column} INTEGER"),
                    &[],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql repair provider_models add {column}: {e}"))?;
        }
    }
    for column in [
        "thinking_supported",
        "thinking_adaptive_supported",
        "thinking_enabled_supported",
    ] {
        if !cols.contains(column) {
            client
                .execute(
                    &format!("ALTER TABLE provider_models ADD COLUMN {column} INTEGER"),
                    &[],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql repair provider_models add {column}: {e}"))?;
        }
    }
    // `max_input_tokens` was merged into `context_window` — the context window
    // *is* the input allowance. Backfill before dropping: Claude/Gemini rows
    // kept their limit here with `context_window` NULL. Conditional on the
    // column existing, so a DB already at the current baseline is a no-op.
    if cols.contains("max_input_tokens") {
        client
            .execute(
                "UPDATE provider_models SET context_window = max_input_tokens \
                 WHERE context_window IS NULL AND max_input_tokens IS NOT NULL",
                &[],
            )
            .await
            .map_err(|e| anyhow::anyhow!("libsql repair provider_models backfill: {e}"))?;
        client
            .execute(
                "ALTER TABLE provider_models DROP COLUMN max_input_tokens",
                &[],
            )
            .await
            .map_err(|e| anyhow::anyhow!("libsql repair provider_models drop: {e}"))?;
    }
    Ok(())
}

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
    if !cols.is_empty() && !cols.contains("image_output_tokens") {
        client
            .execute(
                "ALTER TABLE usages ADD COLUMN image_output_tokens INTEGER NOT NULL DEFAULT 0",
                &[],
            )
            .await
            .map_err(|e| anyhow::anyhow!("libsql repair usages add image output column: {e}"))?;
    }

    let qr = client
        .execute("PRAGMA table_info(usage_rollups)", &[])
        .await
        .map_err(|e| anyhow::anyhow!("libsql inspect usage_rollups columns failed: {e}"))?;
    let rollup_cols = qr
        .rows
        .iter()
        .map(|row| col_str(row, 1))
        .collect::<anyhow::Result<HashSet<_>>>()?;
    if !rollup_cols.is_empty() && !rollup_cols.contains("image_output_tokens") {
        client
            .execute(
                "ALTER TABLE usage_rollups ADD COLUMN image_output_tokens INTEGER NOT NULL DEFAULT 0",
                &[],
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!("libsql repair usage_rollups add image output column: {e}")
            })?;
    }
    Ok(())
}

pub(super) async fn quotas(client: &LibsqlClient) -> anyhow::Result<()> {
    let qr = client
        .execute("PRAGMA table_info(quotas)", &[])
        .await
        .map_err(|e| anyhow::anyhow!("libsql inspect quotas columns failed: {e}"))?;
    let cols = qr
        .rows
        .iter()
        .map(|row| col_str(row, 1))
        .collect::<anyhow::Result<HashSet<_>>>()?;
    if cols.is_empty() {
        return Ok(());
    }
    for (column, definition) in [
        ("quota_daily", "TEXT"),
        ("quota_weekly", "TEXT"),
        ("quota_monthly", "TEXT"),
        ("quota_5h", "TEXT"),
        ("quota_7d", "TEXT"),
        ("day_used", "TEXT NOT NULL DEFAULT '0'"),
        ("day_anchor", "INTEGER NOT NULL DEFAULT 0"),
        ("week_used", "TEXT NOT NULL DEFAULT '0'"),
        ("week_anchor", "INTEGER NOT NULL DEFAULT 0"),
        ("month_used", "TEXT NOT NULL DEFAULT '0'"),
        ("month_anchor", "INTEGER NOT NULL DEFAULT 0"),
        ("five_hour_used", "TEXT NOT NULL DEFAULT '0'"),
        ("five_hour_anchor", "INTEGER NOT NULL DEFAULT 0"),
        ("seven_day_used", "TEXT NOT NULL DEFAULT '0'"),
        ("seven_day_anchor", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !cols.contains(column) {
            client
                .execute(
                    &format!("ALTER TABLE quotas ADD COLUMN {column} {definition}"),
                    &[],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql repair quotas add {column}: {e}"))?;
        }
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
    let had_image_price = cols.contains("image_price");
    let mut changed = false;

    for col in [
        "input_price",
        "output_price",
        "cache_read_price",
        "cache_creation_5m_price",
        "cache_creation_30m_price",
        "cache_creation_1h_price",
        "image_output_price",
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
    if !cols.contains("pricing_tiers_json") {
        client
            .execute(
                "ALTER TABLE price_rules ADD COLUMN pricing_tiers_json TEXT",
                &[],
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!("libsql repair price_rules add pricing_tiers_json: {e}")
            })?;
        changed = true;
    }

    if changed && had_rates_json {
        let sql = "UPDATE price_rules SET \
                input_price = COALESCE(CAST(json_extract(rates_json, '$.input_tokens') AS TEXT), CAST(json_extract(rates_json, '$.input') AS TEXT), input_price), \
                output_price = COALESCE(CAST(json_extract(rates_json, '$.output_tokens') AS TEXT), CAST(json_extract(rates_json, '$.output') AS TEXT), output_price), \
                cache_read_price = COALESCE(CAST(json_extract(rates_json, '$.cache_read_tokens') AS TEXT), CAST(json_extract(rates_json, '$.cache_read') AS TEXT), cache_read_price), \
                cache_creation_5m_price = COALESCE(CAST(json_extract(rates_json, '$.cache_write_tokens') AS TEXT), CAST(json_extract(rates_json, '$.cache_creation') AS TEXT), cache_creation_5m_price), \
                cache_creation_1h_price = COALESCE(CAST(json_extract(rates_json, '$.cache_write_tokens') AS TEXT), CAST(json_extract(rates_json, '$.cache_creation') AS TEXT), cache_creation_1h_price) \
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

    if had_rates_json
        || had_cache_write_price
        || had_operation
        || had_kind
        || had_priority
        || had_image_price
    {
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
            image_output_price TEXT NOT NULL DEFAULT '0', \
            pricing_tiers_json TEXT, \
            enabled INTEGER NOT NULL, \
            created_at INTEGER NOT NULL, \
            updated_at INTEGER NOT NULL)",
        "INSERT INTO price_rules_repaired \
            (id, provider_id, match_type, model_match, \
             input_price, output_price, cache_read_price, cache_creation_5m_price, \
             cache_creation_30m_price, cache_creation_1h_price, image_output_price, \
             pricing_tiers_json, enabled, created_at, updated_at) \
         SELECT \
            id, provider_id, match_type, model_match, \
            input_price, output_price, cache_read_price, cache_creation_5m_price, \
            cache_creation_30m_price, cache_creation_1h_price, image_output_price, \
            pricing_tiers_json, enabled, created_at, updated_at \
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
