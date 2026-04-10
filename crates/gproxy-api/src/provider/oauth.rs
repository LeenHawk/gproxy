use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use gproxy_sdk::provider::engine::UpstreamRequestMeta;
use gproxy_sdk::provider::store::{CredentialSnapshot, CredentialUpdate};
use gproxy_server::AppState;
use gproxy_storage::repository::CredentialRepository;
use gproxy_storage::{CredentialWrite, ProviderQuery, Scope};

use crate::auth::authorize_admin;
use crate::error::HttpError;

async fn persist_oauth_credential(
    state: &AppState,
    provider_name: &str,
    credential: &CredentialSnapshot,
) -> Result<i64, HttpError> {
    let provider = state
        .storage()
        .list_providers(&ProviderQuery {
            name: Scope::Eq(provider_name.to_string()),
            ..Default::default()
        })
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?
        .into_iter()
        .next()
        .ok_or_else(|| HttpError::not_found(format!("provider '{provider_name}' not found")))?;

    let credential_json = credential.credential.to_string();
    let credential_id = state
        .storage()
        .create_credential(provider.id, None, &provider.channel, &credential_json, true)
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    state.append_provider_credential_id_in_memory(provider_name, credential_id);
    Ok(credential_id)
}

/// Start an OAuth flow for a provider.
pub async fn oauth_start(
    State(state): State<Arc<AppState>>,
    Path(provider_name): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    authorize_admin(&headers, &state)?;
    let params = parse_query_string(query.as_deref());

    let result = state.engine().oauth_start(&provider_name, params).await?;

    match result {
        Some(flow) => json_response(&serde_json::json!({
            "authorize_url": flow.authorize_url,
            "state": flow.state,
            "redirect_uri": flow.redirect_uri,
            "verification_uri": flow.verification_uri,
            "user_code": flow.user_code,
            "mode": flow.mode,
            "scope": flow.scope,
            "instructions": flow.instructions,
        })),
        None => Err(HttpError::not_found(format!(
            "provider '{provider_name}' does not support OAuth"
        ))),
    }
}

/// Handle OAuth callback for a provider.
pub async fn oauth_callback(
    State(state): State<Arc<AppState>>,
    Path(provider_name): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    authorize_admin(&headers, &state)?;
    let params = parse_query_string(query.as_deref());

    let result = state.engine().oauth_finish(&provider_name, params).await?;

    match result {
        Some(finish) => {
            if let Err(error) =
                persist_oauth_credential(&state, &provider_name, &finish.credential).await
            {
                let _ = state
                    .engine()
                    .store()
                    .remove_credential(&provider_name, finish.credential.index);
                return Err(error);
            }
            json_response(&serde_json::json!({
                "credential": finish.credential,
                "details": finish.details,
            }))
        }
        None => Err(HttpError::not_found(format!(
            "provider '{provider_name}' OAuth callback failed"
        ))),
    }
}

/// Query upstream usage/quota for a provider.
pub async fn upstream_usage(
    State(state): State<Arc<AppState>>,
    Path(provider_name): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    authorize_admin(&headers, &state)?;
    let params = parse_query_string(query.as_deref());
    let credential_index = resolve_quota_credential_index(&state, &provider_name, &params)?;
    let result = state
        .engine()
        .query_quota(&provider_name, credential_index)
        .await;

    // Always log upstream request and persist credential updates, even on error
    match &result {
        Ok((_, credential_updates, meta)) => {
            persist_credential_updates(&state, credential_updates).await;
            record_internal_upstream_log(&state, &provider_name, meta.as_ref()).await;
        }
        Err(_) => {}
    }

    let (result, _, _) = result?;

    match result {
        Some(response) => Ok(Response::builder()
            .status(response.status)
            .header("content-type", "application/json")
            .body(Body::from(response.body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())),
        None => Err(HttpError::not_found(format!(
            "provider '{provider_name}' does not support quota queries"
        ))),
    }
}

fn parse_query_string(query: Option<&str>) -> HashMap<String, String> {
    let Some(query) = query else {
        return HashMap::new();
    };
    url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

fn parse_optional_i64(
    params: &HashMap<String, String>,
    key: &str,
) -> Result<Option<i64>, HttpError> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<i64>()
        .map(Some)
        .map_err(|_| HttpError::bad_request(format!("invalid {key}: expected integer")))
}

fn parse_optional_usize(
    params: &HashMap<String, String>,
    key: &str,
) -> Result<Option<usize>, HttpError> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<usize>()
        .map(Some)
        .map_err(|_| HttpError::bad_request(format!("invalid {key}: expected integer")))
}

fn resolve_quota_credential_index(
    state: &AppState,
    provider_name: &str,
    params: &HashMap<String, String>,
) -> Result<Option<usize>, HttpError> {
    if let Some(credential_id) = parse_optional_i64(params, "credential_id")? {
        let Some((resolved_provider, index)) = state.credential_position_for_id(credential_id)
        else {
            return Err(HttpError::not_found("credential_id not found"));
        };
        if resolved_provider != provider_name {
            return Err(HttpError::bad_request(
                "credential_id does not belong to the requested provider",
            ));
        }
        return Ok(Some(index));
    }

    if let Some(index) = parse_optional_usize(params, "credential_index")? {
        if state
            .credential_id_for_index(provider_name, index)
            .is_none()
        {
            return Err(HttpError::not_found(
                "provider or credential index not found",
            ));
        }
        return Ok(Some(index));
    }

    Ok(None)
}

fn json_response(value: &serde_json::Value) -> Result<Response, HttpError> {
    let body = serde_json::to_vec(value).unwrap_or_default();
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()))
}

/// Persist refreshed credentials to the database.
///
/// Called after the engine refreshes OAuth tokens so the new tokens survive
/// a restart. Errors are logged but do not fail the request.
pub(crate) async fn persist_credential_updates(state: &AppState, updates: &[CredentialUpdate]) {
    for update in updates {
        let Some(credential_id) = state.credential_id_for_index(&update.provider, update.index)
        else {
            continue;
        };
        let Some(provider_id) = state.provider_id_for_name(&update.provider) else {
            continue;
        };
        let kind = state
            .provider_channel_for_name(&update.provider)
            .unwrap_or_default();

        if let Err(e) = state
            .storage()
            .upsert_credential(CredentialWrite {
                id: credential_id,
                provider_id,
                name: None,
                kind,
                secret_json: update.credential.to_string(),
                enabled: true,
            })
            .await
        {
            tracing::warn!(
                provider = %update.provider,
                credential_index = update.index,
                error = %e,
                "failed to persist refreshed credential to DB"
            );
        } else {
            tracing::info!(
                provider = %update.provider,
                credential_index = update.index,
                "persisted refreshed credential to DB"
            );
        }
    }
}

/// Record an internal upstream request (quota, OAuth, cookie exchange).
pub(crate) async fn record_internal_upstream_log(
    state: &AppState,
    provider_name: &str,
    meta: Option<&UpstreamRequestMeta>,
) {
    let config = state.config();
    if !config.enable_upstream_log {
        return;
    }
    let Some(meta) = meta else {
        return;
    };
    let include_body = config.enable_upstream_log_body;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let provider_id = state.provider_id_for_name(provider_name);
    let credential_id = meta
        .credential_index
        .and_then(|index| state.credential_id_for_index(provider_name, index));
    let _ = state
        .storage()
        .apply_write_event(gproxy_storage::StorageWriteEvent::UpsertUpstreamRequest(
            gproxy_storage::UpstreamRequestWrite {
                downstream_trace_id: None,
                at_unix_ms: now_ms,
                internal: true,
                provider_id,
                credential_id,
                request_method: meta.method.clone(),
                request_headers_json: serde_json::to_string(&meta.request_headers)
                    .unwrap_or_else(|_| "[]".to_string()),
                request_url: Some(meta.url.clone()),
                request_body: if include_body {
                    meta.request_body.clone()
                } else {
                    None
                },
                response_status: meta.response_status.map(|s| s as i32),
                response_headers_json: serde_json::to_string(&meta.response_headers)
                    .unwrap_or_else(|_| "[]".to_string()),
                response_body: if include_body {
                    meta.response_body.clone()
                } else {
                    None
                },
            },
        ))
        .await;
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use gproxy_sdk::provider::store::CredentialSnapshot;
    use gproxy_server::{AppStateBuilder, GlobalConfig};
    use gproxy_storage::{CredentialQuery, SeaOrmStorage};

    use super::{parse_query_string, persist_oauth_credential, resolve_quota_credential_index};

    #[test]
    fn parse_query_string_decodes_percent_encoded_values() {
        let parsed = parse_query_string(Some(
            "callback_url=https%3A%2F%2Flocalhost%2Fcb%3Fcode%3Dabc%26state%3Dxyz&mode=authorization_code",
        ));

        assert_eq!(
            parsed,
            HashMap::from([
                (
                    "callback_url".to_string(),
                    "https://localhost/cb?code=abc&state=xyz".to_string(),
                ),
                ("mode".to_string(), "authorization_code".to_string()),
            ])
        );
    }

    #[tokio::test]
    async fn persist_oauth_credential_writes_db_and_memory_index() {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");
        let provider_id = storage
            .create_provider(
                "demo",
                "codex",
                "{\"base_url\":\"https://chatgpt.com/backend-api/codex\"}",
                "{}",
            )
            .await
            .expect("seed provider");

        let state = AppStateBuilder::new()
            .engine(gproxy_sdk::provider::engine::GproxyEngine::builder().build())
            .storage(storage.clone())
            .config(GlobalConfig {
                dsn: "sqlite::memory:".to_string(),
                ..GlobalConfig::default()
            })
            .build();

        let credential = CredentialSnapshot {
            provider: "demo".to_string(),
            index: 0,
            revision: 0,
            credential: serde_json::json!({
                "access_token": "token",
                "account_id": "fdc791c5-acf2-4760-b8e7-4af508952763",
                "expires_at_ms": 1776493967337u64,
            }),
        };

        let credential_id = persist_oauth_credential(&state, "demo", &credential)
            .await
            .expect("persist oauth credential");

        let saved = storage
            .list_credentials(&CredentialQuery::default())
            .await
            .expect("query credentials");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].id, credential_id);
        assert_eq!(saved[0].provider_id, provider_id);
        assert_eq!(
            state.provider_credential_ids_for("demo"),
            Some(vec![credential_id])
        );
    }

    #[tokio::test]
    async fn resolve_quota_credential_index_prefers_credential_id() {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        let state = AppStateBuilder::new()
            .engine(gproxy_sdk::provider::engine::GproxyEngine::builder().build())
            .storage(storage)
            .config(GlobalConfig {
                dsn: "sqlite::memory:".to_string(),
                ..GlobalConfig::default()
            })
            .build();
        state.replace_provider_credentials(HashMap::from([("demo".to_string(), vec![1000, 1001])]));

        let index = resolve_quota_credential_index(
            &state,
            "demo",
            &HashMap::from([("credential_id".to_string(), "1001".to_string())]),
        )
        .expect("resolve credential id");

        assert_eq!(index, Some(1));
    }

    #[tokio::test]
    async fn resolve_quota_credential_index_validates_provider_membership() {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        let state = AppStateBuilder::new()
            .engine(gproxy_sdk::provider::engine::GproxyEngine::builder().build())
            .storage(storage)
            .config(GlobalConfig {
                dsn: "sqlite::memory:".to_string(),
                ..GlobalConfig::default()
            })
            .build();
        state.replace_provider_credentials(HashMap::from([("other".to_string(), vec![1000])]));

        let error = resolve_quota_credential_index(
            &state,
            "demo",
            &HashMap::from([("credential_id".to_string(), "1000".to_string())]),
        )
        .expect_err("provider mismatch");

        assert_eq!(error.status, axum::http::StatusCode::BAD_REQUEST);
    }
}
