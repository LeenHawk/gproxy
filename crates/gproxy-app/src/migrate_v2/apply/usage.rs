use gproxy_store::records::*;
use rust_decimal::Decimal;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

use super::{Context, id, mark, unsigned};
use crate::migrate_v2::model::{Legacy, SourceData, Usage};
use crate::migrate_v2::report::ImportCount;

pub(super) async fn settings(
    context: &Context<'_>,
    data: &SourceData,
    counts: &mut [ImportCount],
) -> Result<(), crate::AppError> {
    let Some(settings) = data.settings.first().map(|value| &value.value) else {
        return Ok(());
    };
    let values = vec![
        setting(INSTANCE_NAME, Value::String(settings.instance_name.clone())),
        setting(PROXY, option_text(settings.proxy.as_deref())),
        setting(ENABLE_USAGE, Value::Bool(settings.enable_usage)),
        setting(
            ENABLE_TOKENIZER_DOWNLOAD,
            Value::Bool(settings.enable_tokenizer_download),
        ),
        setting(
            FILE_UPLOAD_MAX_IN_FLIGHT,
            Value::from(settings.file_upload_max_in_flight),
        ),
        setting(INHERIT_SYSTEM_PROXY, Value::Bool(false)),
        setting(
            UPDATE_CHANNEL,
            option_text(settings.update_channel.as_deref()),
        ),
        setting(
            ENABLE_AUTO_UPDATE_CHECK,
            Value::Bool(settings.enable_auto_update_check),
        ),
        setting(RETENTION_DAYS, positive(settings.retention_days)),
        setting(
            MAX_DATABASE_SIZE_MB,
            positive(settings.max_database_size_mb),
        ),
        setting(
            ENABLE_DOWNSTREAM_LOG,
            Value::Bool(settings.enable_downstream_log),
        ),
        setting(
            ENABLE_DOWNSTREAM_LOG_BODY,
            Value::Bool(settings.enable_downstream_log_body),
        ),
        setting(
            ENABLE_UPSTREAM_LOG,
            Value::Bool(settings.enable_upstream_log),
        ),
        setting(
            ENABLE_UPSTREAM_LOG_BODY,
            Value::Bool(settings.enable_upstream_log_body),
        ),
        setting(
            DISABLE_LOG_REDACTION,
            Value::Bool(settings.disable_log_redaction),
        ),
    ];
    context.store.import_settings(&values).await?;
    mark(counts, "instance_settings", 1);
    Ok(())
}

pub(super) async fn history(
    context: &Context<'_>,
    data: &SourceData,
    counts: &mut [ImportCount],
) -> Result<(), crate::AppError> {
    let tombstone_providers = data
        .usage_tombstone_providers
        .iter()
        .map(|value| value.id)
        .collect::<BTreeSet<_>>();
    let tombstone_credentials = data
        .usage_tombstone_credentials
        .iter()
        .map(|value| value.id)
        .collect::<BTreeSet<_>>();
    let mut imported = 0;
    for chunk in data.usage.chunks(1_000) {
        let rows = chunk
            .iter()
            .map(|value| history_row(context, value, &tombstone_providers, &tombstone_credentials))
            .collect::<Result<Vec<_>, crate::AppError>>()?;
        imported += context.store.import_usage(&rows).await?;
    }
    mark(
        counts,
        "usage",
        usize::try_from(imported).unwrap_or(usize::MAX),
    );
    Ok(())
}

fn history_row(
    context: &Context<'_>,
    value: &Legacy<Usage>,
    tombstone_providers: &BTreeSet<i64>,
    tombstone_credentials: &BTreeSet<i64>,
) -> Result<UsageInput, crate::AppError> {
    let usage = &value.value;
    let mut metrics = usage
        .metrics
        .as_object()
        .cloned()
        .expect("usage metrics were validated");
    metric(
        &mut metrics,
        "image_output_tokens",
        usage.image_output_tokens,
    );
    metric(
        &mut metrics,
        "cache_creation_5m_tokens",
        usage.cache_creation_5m_tokens,
    );
    metric(
        &mut metrics,
        "cache_creation_30m_tokens",
        usage.cache_creation_30m_tokens,
    );
    metric(
        &mut metrics,
        "cache_creation_1h_tokens",
        usage.cache_creation_1h_tokens,
    );
    let mut dimensions = Map::from_iter([
        ("v2_route".into(), Value::from(usage.route_name.clone())),
        ("v2_kind".into(), Value::from(usage.kind.clone())),
        ("v2_thread_id".into(), Value::from(usage.thread_id.clone())),
    ]);
    retain_deleted_reference(
        &mut dimensions,
        "v2_deleted_provider_id",
        usage.provider_id,
        tombstone_providers,
    );
    retain_deleted_reference(
        &mut dimensions,
        "v2_deleted_credential_id",
        usage.credential_id,
        tombstone_credentials,
    );
    retain_missing_reference(
        &mut dimensions,
        "v2_deleted_organization_id",
        usage.organization_id,
        &context.organizations,
    );
    retain_missing_reference(
        &mut dimensions,
        "v2_deleted_team_id",
        usage.team_id,
        &context.teams,
    );
    retain_missing_reference(
        &mut dimensions,
        "v2_deleted_user_id",
        usage.user_id,
        &context.users,
    );
    retain_missing_reference(
        &mut dimensions,
        "v2_deleted_user_key_id",
        usage.user_key_id,
        &context.user_keys,
    );
    Ok(UsageInput {
        upstream_started_at_ms: None,
        request_id: usage.request_id.clone(),
        at: usage.at,
        provider_id: id(&context.providers, required(usage.provider_id, "provider")?)?,
        credential_id: id(
            &context.credentials,
            required(usage.credential_id, "credential")?,
        )?,
        organization_id: retained(&context.organizations, usage.organization_id),
        team_id: retained(&context.teams, usage.team_id),
        user_id: retained(&context.users, usage.user_id),
        user_key_id: retained(&context.user_keys, usage.user_key_id),
        operation: Some(usage.operation.clone()),
        upstream_model: usage.model.clone().unwrap_or_default(),
        input_tokens: unsigned(usage.input_tokens, "input tokens")?,
        output_tokens: unsigned(usage.output_tokens, "output tokens")?,
        cached_input_tokens: unsigned(usage.cache_read_tokens, "cache tokens")?,
        metrics: Value::Object(metrics),
        dimensions: Value::Object(dimensions),
        cost: usage.cost,
        usage_source: usage.usage_source.clone(),
        ended: usage.ended.clone(),
        latency_ms: unsigned(usage.latency_ms, "latency")?,
    })
}

fn retained(map: &std::collections::BTreeMap<i64, i64>, old: Option<i64>) -> Option<i64> {
    old.and_then(|id| map.get(&id).copied())
}

fn retain_missing_reference(
    dimensions: &mut Map<String, Value>,
    name: &str,
    old: Option<i64>,
    map: &std::collections::BTreeMap<i64, i64>,
) {
    if let Some(id) = old.filter(|id| !map.contains_key(id)) {
        dimensions.insert(name.into(), Value::from(id));
    }
}

fn retain_deleted_reference(
    dimensions: &mut Map<String, Value>,
    name: &str,
    old: Option<i64>,
    tombstones: &BTreeSet<i64>,
) {
    if let Some(id) = old.filter(|id| tombstones.contains(id)) {
        dimensions.insert(name.into(), Value::from(id));
    }
}

fn setting(key: &str, value: Value) -> SettingInput {
    SettingInput {
        key: key.into(),
        value,
    }
}

fn option_text(value: Option<&str>) -> Value {
    value
        .filter(|value| !value.trim().is_empty())
        .map(Value::from)
        .unwrap_or(Value::Null)
}

fn positive(value: Option<i64>) -> Value {
    value
        .filter(|value| *value > 0)
        .map(Value::from)
        .unwrap_or(Value::Null)
}

fn metric(metrics: &mut Map<String, Value>, name: &str, value: i64) {
    if value != 0 {
        metrics
            .entry(name)
            .or_insert_with(|| Value::String(Decimal::from(value).to_string()));
    }
}

fn required(value: Option<i64>, field: &str) -> Result<i64, crate::AppError> {
    value.ok_or_else(|| crate::AppError::Migration(format!("missing {field} after validation")))
}
