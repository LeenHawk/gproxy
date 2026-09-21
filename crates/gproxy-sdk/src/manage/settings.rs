//! The single settings row.
//!
//! One durable row, two groups in the API: instance configuration and logging.
//! The update goes into the revision batch through `Settings::update_statement`
//! rather than through `Settings::update`, so the row change and the revision
//! bump are the same transaction — and only one bump, because
//! `commit_revision` already does it.

use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::entity::config::setting;
use sea_orm::Set;

use super::{Scope, Writer, crud};
use crate::{
    SdkError, SdkResult,
    dto::{SettingsDto, SettingsPatch},
};

pub struct SettingsManage<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> SettingsManage<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> SettingsManage<'_, C> {
    pub async fn get(&self) -> SdkResult<SettingsDto> {
        let row = self
            .writer
            .store()
            .settings()
            .get()
            .await?
            .ok_or_else(|| SdkError::invalid("the global settings row is missing"))?;
        Ok(row.into())
    }

    pub async fn update(&self, patch: SettingsPatch) -> SdkResult<SettingsDto> {
        let mut row = setting::ActiveModel {
            id: Set(setting::GLOBAL_SETTINGS_ID),
            ..Default::default()
        };
        if let Some(instance) = patch.instance {
            if let Some(value) = instance.instance_name {
                row.instance_name = Set(crud::text(&value, "instanceName")?);
            }
            if let Some(value) = instance.oauth_client_allowlist {
                row.oauth_client_allowlist = Set(array(value, "oauthClientAllowlist")?);
            }
            if let Some(value) = instance.connection_profile_id {
                let value = crud::optional_text(value);
                if let Some(id) = &value {
                    crud::require_rows(
                        self.writer.store().connection_profiles(),
                        "connection profile",
                        std::slice::from_ref(id),
                    )
                    .await?;
                }
                row.connection_profile_id = Set(value);
            }
            if let Some(value) = instance.cors_origins {
                row.cors_origins =
                    Set(array(Some(value), "corsOrigins")?.unwrap_or_else(empty_array));
            }
            if let Some(value) = instance.trusted_proxies {
                row.trusted_proxies =
                    Set(array(Some(value), "trustedProxies")?.unwrap_or_else(empty_array));
            }
            if let Some(value) = instance.max_attempts {
                if value == 0 {
                    return Err(SdkError::invalid("maxAttempts must be positive"));
                }
                row.max_attempts = Set(value);
            }
            if let Some(value) = instance.max_in_flight {
                row.max_in_flight = Set(value);
            }
            if let Some(value) = instance.file_upload_max_in_flight {
                row.file_upload_max_in_flight = Set(value);
            }
            if let Some(value) = instance.enable_settlement {
                row.enable_settlement = Set(value);
            }
            if let Some(value) = instance.enable_usage {
                row.enable_usage = Set(value);
            }
            if let Some(value) = instance.request_timeout_ms {
                row.request_timeout_ms = Set(positive(value, "requestTimeoutMs")?);
            }
            if let Some(value) = instance.stream_idle_timeout_ms {
                row.stream_idle_timeout_ms = Set(positive(value, "streamIdleTimeoutMs")?);
            }
            if let Some(value) = instance.max_request_body_bytes {
                row.max_request_body_bytes = Set(positive64(value, "maxRequestBodyBytes")?);
            }
            if let Some(value) = instance.max_response_body_bytes {
                row.max_response_body_bytes = Set(positive64(value, "maxResponseBodyBytes")?);
            }
            if let Some(value) = instance.max_stream_event_bytes {
                row.max_stream_event_bytes = Set(positive64(value, "maxStreamEventBytes")?);
            }
            if let Some(value) = instance.max_ws_frame_bytes {
                row.max_ws_frame_bytes = Set(positive64(value, "maxWsFrameBytes")?);
            }
            if let Some(value) = instance.max_multipart_parts {
                row.max_multipart_parts = Set(positive(value, "maxMultipartParts")?);
            }
            if let Some(value) = instance.enable_tokenizer_vocabs {
                row.enable_tokenizer_vocabs = Set(value);
            }
            if let Some(value) = instance.enable_tokenizer_download {
                row.enable_tokenizer_download = Set(value);
            }
            if let Some(value) = instance.default_vocabulary_file_id {
                let value = crud::optional_text(value);
                if let Some(id) = &value {
                    crud::require_rows(
                        self.writer.store().file_objects(),
                        "file object",
                        std::slice::from_ref(id),
                    )
                    .await?;
                }
                row.default_vocabulary_file_id = Set(value);
            }
            if let Some(value) = instance.tokenizer_auth_token {
                // Sealed exactly like a credential secret, under this row's
                // own identity, so a database copy carries no usable token.
                row.tokenizer_auth_token = Set(match crud::optional_text(value) {
                    Some(token) => Some(
                        self.writer
                            .core()
                            .secret_codec()
                            .seal(TOKENIZER_SECRET_ID, &serde_json::json!({ "token": token }))?,
                    ),
                    None => None,
                });
            }
            if let Some(value) = instance.default_file_storage_name {
                row.default_file_storage_name = Set(crud::optional_text(value));
            }
            if let Some(value) = instance.retention_days {
                row.retention_days = Set(value);
            }
            if let Some(value) = instance.max_database_size_mb {
                if value.is_some_and(|size| size < 0) {
                    return Err(SdkError::invalid("maxDatabaseSizeMb must not be negative"));
                }
                row.max_database_size_mb = Set(value);
            }
            if let Some(value) = instance.update_channel {
                row.update_channel = Set(crud::optional_text(value));
            }
            if let Some(value) = instance.enable_auto_update_check {
                row.enable_auto_update_check = Set(value);
            }
            if let Some(value) = instance.portal_recent_requests_enabled {
                row.portal_recent_requests_enabled = Set(value);
            }
        }
        if let Some(logging) = patch.logging {
            if let Some(value) = logging.enable_downstream_log {
                row.enable_downstream_log = Set(value);
            }
            if let Some(value) = logging.enable_downstream_log_body {
                row.enable_downstream_log_body = Set(value);
            }
            if let Some(value) = logging.enable_upstream_log {
                row.enable_upstream_log = Set(value);
            }
            if let Some(value) = logging.enable_upstream_log_body {
                row.enable_upstream_log_body = Set(value);
            }
            if let Some(value) = logging.disable_log_redaction {
                row.disable_log_redaction = Set(value);
            }
            if let Some(value) = logging.enable_tracing {
                row.enable_tracing = Set(value);
            }
            if let Some(value) = logging.log_level {
                row.log_level = Set(crud::text(&value, "logLevel")?.to_ascii_lowercase());
            }
            if let Some(value) = logging.log_format {
                row.log_format = Set(crud::text(&value, "logFormat")?.to_ascii_lowercase());
            }
            if let Some(value) = logging.request_header_blacklist {
                row.request_header_blacklist =
                    Set(array(Some(value), "requestHeaderBlacklist")?.unwrap_or_else(empty_array));
            }
            if let Some(value) = logging.response_header_blacklist {
                row.response_header_blacklist =
                    Set(array(Some(value), "responseHeaderBlacklist")?.unwrap_or_else(empty_array));
            }
            if let Some(value) = logging.query_parameter_blacklist {
                row.query_parameter_blacklist =
                    Set(array(Some(value), "queryParameterBlacklist")?.unwrap_or_else(empty_array));
            }
        }

        // Read back after the commit rather than inside it. The caller's
        // statements run before `commit_revision` bumps the counter, so a
        // read-back in the same batch would report the settings row with the
        // revision it had *before* its own write — the one number of this row
        // that a caller would notice being stale.
        let statements = vec![BatchStatement::Execute(
            self.writer.store().settings().update_statement(row)?,
        )];
        self.writer.commit(statements, &[Scope::Settings]).await?;
        self.get().await
    }
}

/// The settings row has no credential id of its own; this constant binds its
/// sealed token to the row instead. `manage::tokenizer` seals the same column,
/// so it reads this one rather than keeping a copy that could drift.
pub(super) const TOKENIZER_SECRET_ID: &str = "settings:tokenizer_auth_token";

fn empty_array() -> serde_json::Value {
    serde_json::Value::Array(Vec::new())
}

/// A JSON string array column. `null` clears it; anything that is not an array
/// would be read as an empty list wherever it is consumed.
fn array(
    value: Option<serde_json::Value>,
    field: &'static str,
) -> SdkResult<Option<serde_json::Value>> {
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) if value.is_array() => Ok(Some(value)),
        Some(_) => Err(SdkError::invalid(format!("{field} must be a JSON array"))),
    }
}

fn positive(value: u32, field: &'static str) -> SdkResult<u32> {
    if value == 0 {
        return Err(SdkError::invalid(format!("{field} must be positive")));
    }
    Ok(value)
}

fn positive64(value: i64, field: &'static str) -> SdkResult<i64> {
    if value <= 0 {
        return Err(SdkError::invalid(format!("{field} must be positive")));
    }
    Ok(value)
}
