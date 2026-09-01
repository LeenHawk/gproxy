use gproxy_store::records::*;
use rust_decimal::Decimal;
use serde_json::{Map, Value};

use super::{Context, id, mark, optional, unsigned};
use crate::migrate_v2::model::SourceData;
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
    let rows = data
        .usage
        .iter()
        .map(|value| {
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
            let dimensions = serde_json::json!({
                "v2_route": usage.route_name,
                "v2_kind": usage.kind,
                "v2_thread_id": usage.thread_id,
            });
            Ok(UsageInput {
                request_id: usage.request_id.clone(),
                at: usage.at,
                provider_id: id(&context.providers, required(usage.provider_id, "provider")?)?,
                credential_id: id(
                    &context.credentials,
                    required(usage.credential_id, "credential")?,
                )?,
                organization_id: optional(&context.organizations, usage.organization_id)?,
                team_id: optional(&context.teams, usage.team_id)?,
                user_id: optional(&context.users, usage.user_id)?,
                user_key_id: optional(&context.user_keys, usage.user_key_id)?,
                operation: Some(usage.operation.clone()),
                upstream_model: usage.model.clone().unwrap_or_default(),
                input_tokens: unsigned(usage.input_tokens, "input tokens")?,
                output_tokens: unsigned(usage.output_tokens, "output tokens")?,
                cached_input_tokens: unsigned(usage.cache_read_tokens, "cache tokens")?,
                metrics: Value::Object(metrics),
                dimensions,
                cost: usage.cost,
                usage_source: usage.usage_source.clone(),
                ended: usage.ended.clone(),
                latency_ms: unsigned(usage.latency_ms, "latency")?,
            })
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    let imported = context.store.import_usage(&rows).await?;
    mark(
        counts,
        "usage",
        usize::try_from(imported).unwrap_or(usize::MAX),
    );
    Ok(())
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
