//! Stream historical usage into the ledger, without pricing or settlement.
use std::{collections::HashSet, sync::Arc};

use crate::App;
use gproxy_seaorm::{BatchConnectionTrait, FixedDecimal};
use gproxy_store::entity::usage::usage_record;
use rust_decimal::{Decimal, prelude::ToPrimitive};
use sea_orm::{QueryResult, Set};
use serde_json::{Value, json};

use super::{Error, Result};
use super::{Report, ids, source};

pub async fn import<C, S: BatchConnectionTrait>(
    app: &Arc<App<C>>,
    source: &source::Source<'_, S>,
    report: &mut Report,
) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    if !source.tables.iter().any(|t| t == "usage_rows") {
        return Ok(());
    }
    let repository = app.gproxy().store().usage_records();
    let mut cursor: Option<i64> = None;
    let mut imported = 0;
    let mut rounded = 0;
    loop {
        let rows = source
            .rows(
                "usage_rows",
                &["id"],
                cursor.map(|id| ("id", id)),
                Some(256),
            )
            .await?;
        if rows.is_empty() {
            break;
        }
        let mut batch = Vec::with_capacity(rows.len());
        for row in rows {
            let id: i64 = row.try_get("", "id")?;
            cursor = Some(id);
            let (record, was_rounded) = translate(&row)
                .map_err(|error| Error::other(format!("v3 usage_rows {id}: {error}")))?;
            rounded += u64::from(was_rounded);
            batch.push(record);
        }
        // An interrupted import may have committed earlier batches already.
        let keys: Vec<_> = batch
            .iter()
            .map(|row| row.request_id.as_ref().clone())
            .collect();
        let existing: HashSet<_> = repository
            .get_many(&keys)
            .await?
            .into_iter()
            .flatten()
            .map(|row| row.request_id)
            .collect();
        batch.retain(|row| !existing.contains(row.request_id.as_ref()));
        imported += batch.len() as u64;
        repository.insert_many(batch).await?;
    }
    report.count("usage_records", imported);
    if rounded > 0 {
        report.warn(format!("{rounded} historical usage costs rounded to v4's 9-decimal scale; exact originals remain in metrics.v3.cost"));
    }
    Ok(())
}

fn decimal(value: &Value) -> Option<Decimal> {
    let text = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    Decimal::from_str_exact(&text).ok()
}

pub(crate) fn translate(row: &QueryResult) -> Result<(usage_record::ActiveModel, bool)> {
    let id: i64 = row.try_get("", "id")?;
    let at: i64 = row.try_get("", "at")?;
    let latency: i64 = row.try_get("", "latency_ms")?;
    // Older v3 schemas did not record upstream start time.
    let start: Option<i64> = row.try_get("", "upstream_started_at_ms").unwrap_or(None);
    let ended_at = match start {
        Some(start) => start.checked_add(latency),
        None => at.checked_mul(1000),
    }
    .ok_or_else(|| Error::other("usage timestamp overflow"))?;
    let started_at = start
        .or_else(|| ended_at.checked_sub(latency))
        .ok_or_else(|| Error::other("usage timestamp overflow"))?;
    let raw_metrics: String = row.try_get("", "metrics_json")?;
    let raw_dimensions: String = row.try_get("", "dimensions_json")?;
    let metrics: Value =
        serde_json::from_str(&raw_metrics).map_err(|e| Error::other(e.to_string()))?;
    let dimensions: Value =
        serde_json::from_str(&raw_dimensions).map_err(|e| Error::other(e.to_string()))?;
    let mut quantities = metrics
        .as_object()
        .cloned()
        .ok_or_else(|| Error::other("metrics_json is not an object"))?;
    let original_cost: String = row.try_get("", "cost")?;
    let cost = Decimal::from_str_exact(&original_cost).map_err(|e| Error::other(e.to_string()))?;
    let stored_cost = FixedDecimal::rounded(cost).map_err(|e| Error::other(e.to_string()))?;
    let ended: String = row.try_get("", "ended")?;
    let usage_source: String = row.try_get("", "usage_source")?;
    let attribution = |column, table| -> Result<Option<String>> {
        Ok(row
            .try_get::<Option<i64>>("", column)?
            .map(|id| ids::id(table, id)))
    };
    let operation: Option<String> = row.try_get("", "operation")?;
    let mut out = usage_record::ActiveModel {
        request_id: Set(ids::id("usage_rows", id)),
        user_id: Set(attribution("user_id", "users")?),
        api_key_id: Set(attribution("user_key_id", "user_keys")?),
        provider_id: Set(attribution("provider_id", "providers")?),
        credential_id: Set(attribution("credential_id", "credentials")?),
        model: Set(row.try_get("", "upstream_model")?),
        operation: Set(operation.clone().unwrap_or_else(|| "unknown".into())),
        started_at_ms: Set(started_at),
        ended_at_ms: Set(Some(ended_at)),
        input_tokens: Set(Some(row.try_get("", "input_tokens")?)),
        output_tokens: Set(Some(row.try_get("", "output_tokens")?)),
        cached_input_tokens: Set(Some(row.try_get("", "cached_input_tokens")?)),
        state: Set(Some(
            match ended.as_str() {
                "complete" => "completed",
                "interrupted" => "failed",
                other => other,
            }
            .to_owned(),
        )),
        completeness: Set(Some(
            match (usage_source.as_str(), ended.as_str()) {
                ("upstream", "complete") => "complete",
                ("upstream", _) => "partial",
                _ => "unknown",
            }
            .into(),
        )),
        actual_service_tier: Set(dimensions
            .get("service_tier")
            .and_then(Value::as_str)
            .map(str::to_owned)),
        cost: Set(Some(stored_cost)),
        ..Default::default()
    };
    macro_rules! tokens { ($($field:ident),* $(,)?) => {$({
        let key = stringify!($field);
        if let Some(value) = quantities.get(key).and_then(decimal)
            .filter(|v| v.fract().is_zero()).and_then(|v| v.to_i64()) {
            out.$field = Set(Some(value));
            quantities.remove(key);
        }
    })*}; }
    tokens!(
        reasoning_tokens,
        cache_creation_5m_tokens,
        cache_creation_30m_tokens,
        cache_creation_1h_tokens
    );
    macro_rules! fixed { ($($field:ident),* $(,)?) => {$({
        let key = stringify!($field);
        if let Some(value) = quantities.get(key).and_then(decimal).and_then(|v| FixedDecimal::exact(v).ok()) {
            out.$field = Set(Some(value));
            quantities.remove(key);
        }
    })*}; }
    fixed!(
        image_input_tokens,
        image_output_tokens,
        image_outputs,
        audio_input_tokens,
        cached_audio_input_tokens,
        audio_output_tokens,
        audio_seconds,
        audio_characters,
        video_input_tokens,
        video_tokens,
        video_seconds,
        video_outputs,
        search_units,
        web_searches,
        web_fetches,
        file_searches,
        code_interpreter_sessions,
        tool_calls,
        requests
    );
    out.metrics = Set(json!({
        "metrics": quantities, "dimensions": dimensions,
        "v3": {
            "id": id, "request_id": row.try_get::<String>("", "request_id")?,
            "at": at, "upstream_started_at_ms": start, "latency_ms": latency,
            "organization_id": attribution("organization_id", "organizations")?,
            "team_id": attribution("team_id", "teams")?,
            "operation": operation, "ended": ended, "usage_source": usage_source, "cost": original_cost,
            "metrics": metrics,
        }
    }));
    Ok((out, cost != stored_cost.decimal()))
}
