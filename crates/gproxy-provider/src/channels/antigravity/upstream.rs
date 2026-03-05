use std::time::{SystemTime, UNIX_EPOCH};

use gproxy_middleware::{OperationFamily, ProtocolKind, TransformRequest, TransformResponse};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};
use wreq::{Client as WreqClient, Method as WreqMethod, Response as WreqResponse};

use super::constants::ANTIGRAVITY_USER_AGENT;
use super::oauth::{
    AntigravityRefreshedToken, antigravity_auth_material_from_credential,
    resolve_antigravity_access_token,
};
use crate::channels::retry::{
    CredentialRetryDecision, cache_affinity_hint_from_transform_request,
    configured_pick_mode_uses_cache, credential_pick_mode, retry_with_eligible_credentials,
    retry_with_eligible_credentials_with_affinity,
};
use crate::channels::upstream::{
    UpstreamCredentialUpdate, UpstreamError, UpstreamRequestMeta, UpstreamResponse,
};
use crate::channels::utils::{
    gemini_model_list_query_string, is_transient_server_failure, join_base_url_and_path,
    resolve_user_agent_or_default, retry_after_to_millis, to_wreq_method,
};
use crate::channels::{BuiltinChannelCredential, ChannelCredential};
use crate::credential::ChannelCredentialStateStore;
use crate::credential_state::CredentialStateManager;
use crate::provider::ProviderDefinition;

type ParsedGeminiPayload = (Option<String>, Option<Value>, Option<String>);

fn is_antigravity_auth_failure(status_code: u16) -> bool {
    status_code == 401
}

#[derive(Debug, Clone)]
enum AntigravityRequestKind {
    ModelList {
        page_size: Option<u32>,
        page_token: Option<String>,
    },
    ModelGet {
        target: String,
    },
    Forward {
        requires_project: bool,
        request_type: Option<&'static str>,
    },
}

#[derive(Debug, Clone)]
struct AntigravityPreparedRequest {
    method: WreqMethod,
    path: String,
    query: Option<String>,
    body: Option<Value>,
    model: Option<String>,
    kind: AntigravityRequestKind,
}

pub async fn execute_antigravity_with_retry(
    client: &WreqClient,
    provider: &ProviderDefinition,
    credential_states: &ChannelCredentialStateStore,
    request: &TransformRequest,
    now_unix_ms: u64,
) -> Result<UpstreamResponse, UpstreamError> {
    if let Some(local_response) = try_local_antigravity_count_response(request)? {
        return Ok(UpstreamResponse::from_local(local_response));
    }

    let prepared = AntigravityPreparedRequest::from_transform_request(request)?;
    let cache_affinity_hint = if configured_pick_mode_uses_cache(provider.credential_pick_mode) {
        crate::channels::retry::cache_affinity_protocol_from_transform_request(request).and_then(
            |protocol| {
                cache_affinity_hint_from_transform_request(
                    protocol,
                    prepared.model.as_deref(),
                    prepared
                        .body
                        .as_ref()
                        .and_then(|value| serde_json::to_vec(value).ok())
                        .as_deref(),
                )
            },
        )
    } else {
        None
    };
    execute_antigravity_with_prepared(
        client,
        provider,
        credential_states,
        prepared,
        now_unix_ms,
        cache_affinity_hint,
    )
    .await
}

pub async fn execute_antigravity_payload_with_retry(
    client: &WreqClient,
    provider: &ProviderDefinition,
    credential_states: &ChannelCredentialStateStore,
    operation: OperationFamily,
    protocol: ProtocolKind,
    body: &[u8],
    now_unix_ms: u64,
) -> Result<UpstreamResponse, UpstreamError> {
    if (operation, protocol) == (OperationFamily::CountToken, ProtocolKind::Gemini) {
        let payload_value = serde_json::from_slice::<Value>(body)
            .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
        let payload = payload_value.get("body").cloned().unwrap_or(payload_value);
        let text = collect_count_text(&payload);
        let total_tokens = (text.chars().count() as u64).div_ceil(4);
        let response_json = json!({
            "stats_code": 200,
            "headers": {},
            "body": {
                "totalTokens": total_tokens,
            }
        });
        let response = serde_json::from_value(response_json)
            .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
        return Ok(UpstreamResponse::from_local(
            TransformResponse::CountTokenGemini(response),
        ));
    }

    let prepared = AntigravityPreparedRequest::from_payload(operation, protocol, body)?;
    execute_antigravity_with_prepared(
        client,
        provider,
        credential_states,
        prepared,
        now_unix_ms,
        None,
    )
    .await
}

async fn execute_antigravity_with_prepared(
    client: &WreqClient,
    provider: &ProviderDefinition,
    credential_states: &ChannelCredentialStateStore,
    prepared: AntigravityPreparedRequest,
    now_unix_ms: u64,
    cache_affinity_hint: Option<crate::channels::retry::CacheAffinityHint>,
) -> Result<UpstreamResponse, UpstreamError> {
    let base_url = provider.settings.base_url().trim();
    if base_url.is_empty() {
        return Err(UpstreamError::InvalidBaseUrl);
    }

    let state_manager = CredentialStateManager::new(now_unix_ms);
    let method_template = prepared.method.clone();
    let path_template = prepared.path.clone();
    let query_template = prepared.query.clone();
    let body_template = prepared.body.clone();
    let model_template = prepared.model.clone();
    let kind_template = prepared.kind.clone();
    let base_url_template = base_url.to_string();
    let user_agent_template =
        resolve_user_agent_or_default(provider.settings.user_agent(), ANTIGRAVITY_USER_AGENT);
    let pick_mode =
        credential_pick_mode(provider.credential_pick_mode, cache_affinity_hint.as_ref());

    retry_with_eligible_credentials_with_affinity(
        provider,
        credential_states,
        prepared.model.as_deref(),
        now_unix_ms,
        pick_mode,
        cache_affinity_hint,
        |credential| {
            if let ChannelCredential::Builtin(BuiltinChannelCredential::Antigravity(value)) =
                &credential.credential
            {
                return antigravity_auth_material_from_credential(value);
            }
            None
        },
        |attempt| {
            let method = method_template.clone();
            let path = path_template.clone();
            let query = query_template.clone();
            let body = body_template.clone();
            let model = model_template.clone();
            let kind = kind_template.clone();
            let base_url = base_url_template.clone();
            let user_agent = user_agent_template.clone();

            async move {
                let path_with_query = match query.as_deref() {
                    Some(query) if !query.is_empty() => format!("{path}?{query}"),
                    _ => path.clone(),
                };
                let url = join_base_url_and_path(base_url.as_str(), path_with_query.as_str());
                let token_cache_key =
                    format!("{}::{}", provider.channel.as_str(), attempt.credential_id);
                let mut credential_update = None;

                let resolved_access_token = match resolve_antigravity_access_token(
                    client,
                    &provider.settings,
                    token_cache_key.as_str(),
                    &attempt.material,
                    now_unix_ms,
                    false,
                )
                .await
                {
                    Ok(token) => token,
                    Err(err) => {
                        let message = err.as_message();
                        if err.is_invalid_credential() {
                            state_manager.mark_auth_dead(
                                credential_states,
                                &provider.channel,
                                attempt.credential_id,
                                Some(message.clone()),
                            );
                        } else {
                            state_manager.mark_transient_failure(
                                credential_states,
                                &provider.channel,
                                attempt.credential_id,
                                model.as_deref(),
                                None,
                                Some(message.clone()),
                            );
                        }
                        return CredentialRetryDecision::Retry {
                            last_status: None,
                            last_error: Some(message),
                            last_request_meta: None,
                        };
                    }
                };
                if let Some(refreshed) = resolved_access_token.refreshed.as_ref() {
                    credential_update = Some(antigravity_credential_update(
                        attempt.credential_id,
                        refreshed,
                    ));
                }

                let session_id = session_id_for_kind(
                    &kind,
                    attempt.credential_id,
                    attempt.material.project_id.as_str(),
                );
                let body_bytes = match build_request_body_bytes(
                    body.as_ref(),
                    model.as_deref(),
                    &kind,
                    attempt.material.project_id.as_str(),
                    session_id.as_deref(),
                ) {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        let message = err.to_string();
                        state_manager.mark_transient_failure(
                            credential_states,
                            &provider.channel,
                            attempt.credential_id,
                            model.as_deref(),
                            None,
                            Some(message.clone()),
                        );
                        return CredentialRetryDecision::Retry {
                            last_status: None,
                            last_error: Some(message),
                            last_request_meta: None,
                        };
                    }
                };
                let request_type = request_type_for_kind(&kind, model.as_deref());
                let mut request_id = make_request_id();
                let (mut response, mut request_meta) = match send_antigravity_request(
                    client,
                    method.clone(),
                    url.as_str(),
                    resolved_access_token.access_token.as_str(),
                    user_agent.as_str(),
                    request_type,
                    session_id.as_deref(),
                    body_bytes.as_deref(),
                    request_id.as_str(),
                )
                .await
                {
                    Ok((response, request_meta)) => (response, request_meta),
                    Err(err) => {
                        let message = err.to_string();
                        state_manager.mark_transient_failure(
                            credential_states,
                            &provider.channel,
                            attempt.credential_id,
                            model.as_deref(),
                            None,
                            Some(message.clone()),
                        );
                        return CredentialRetryDecision::Retry {
                            last_status: None,
                            last_error: Some(message),
                            last_request_meta: None,
                        };
                    }
                };

                let mut status_code = response.status().as_u16();
                if is_antigravity_auth_failure(status_code) {
                    let refreshed_access_token = match resolve_antigravity_access_token(
                        client,
                        &provider.settings,
                        token_cache_key.as_str(),
                        &attempt.material,
                        now_unix_ms,
                        true,
                    )
                    .await
                    {
                        Ok(token) => token,
                        Err(err) => {
                            let message = err.as_message();
                            if err.is_invalid_credential() {
                                state_manager.mark_auth_dead(
                                    credential_states,
                                    &provider.channel,
                                    attempt.credential_id,
                                    Some(message.clone()),
                                );
                            } else {
                                state_manager.mark_transient_failure(
                                    credential_states,
                                    &provider.channel,
                                    attempt.credential_id,
                                    model.as_deref(),
                                    None,
                                    Some(message.clone()),
                                );
                            }
                            return CredentialRetryDecision::Retry {
                                last_status: Some(status_code),
                                last_error: Some(message),
                                last_request_meta: None,
                            };
                        }
                    };
                    if let Some(refreshed) = refreshed_access_token.refreshed.as_ref() {
                        credential_update = Some(antigravity_credential_update(
                            attempt.credential_id,
                            refreshed,
                        ));
                    }
                    request_id = make_request_id();
                    (response, request_meta) = match send_antigravity_request(
                        client,
                        method,
                        url.as_str(),
                        refreshed_access_token.access_token.as_str(),
                        user_agent.as_str(),
                        request_type,
                        session_id.as_deref(),
                        body_bytes.as_deref(),
                        request_id.as_str(),
                    )
                    .await
                    {
                        Ok((response, request_meta)) => (response, request_meta),
                        Err(err) => {
                            let message = err.to_string();
                            state_manager.mark_transient_failure(
                                credential_states,
                                &provider.channel,
                                attempt.credential_id,
                                model.as_deref(),
                                None,
                                Some(message.clone()),
                            );
                            return CredentialRetryDecision::Retry {
                                last_status: None,
                                last_error: Some(message),
                                last_request_meta: None,
                            };
                        }
                    };

                    status_code = response.status().as_u16();
                    if is_antigravity_auth_failure(status_code) {
                        let message = format!(
                            "upstream status {} after antigravity access token refresh",
                            status_code
                        );
                        state_manager.mark_auth_dead(
                            credential_states,
                            &provider.channel,
                            attempt.credential_id,
                            Some(message.clone()),
                        );
                        return CredentialRetryDecision::Retry {
                            last_status: Some(status_code),
                            last_error: Some(message),
                            last_request_meta: None,
                        };
                    }
                }

                if status_code == 429 {
                    let retry_after_ms = retry_after_to_millis(response.headers());
                    let message = format!("upstream status {status_code}");
                    state_manager.mark_rate_limited(
                        credential_states,
                        &provider.channel,
                        attempt.credential_id,
                        model.as_deref(),
                        retry_after_ms,
                        Some(message.clone()),
                    );
                    return CredentialRetryDecision::Retry {
                        last_status: Some(status_code),
                        last_error: Some(message),
                        last_request_meta: None,
                    };
                }

                if is_transient_server_failure(status_code) {
                    let message = format!("upstream status {status_code}");
                    state_manager.mark_transient_failure(
                        credential_states,
                        &provider.channel,
                        attempt.credential_id,
                        model.as_deref(),
                        None,
                        Some(message.clone()),
                    );
                    return CredentialRetryDecision::Retry {
                        last_status: Some(status_code),
                        last_error: Some(message),
                        last_request_meta: None,
                    };
                }

                match &kind {
                    AntigravityRequestKind::Forward { .. } => {
                        if response.status().is_success() {
                            state_manager.mark_success(
                                credential_states,
                                &provider.channel,
                                attempt.credential_id,
                            );
                        }
                        CredentialRetryDecision::Return(
                            UpstreamResponse::from_http(
                                attempt.credential_id,
                                attempt.attempts,
                                response,
                            )
                            .with_request_meta(request_meta)
                            .with_credential_update(credential_update.clone()),
                        )
                    }
                    AntigravityRequestKind::ModelList {
                        page_size,
                        page_token,
                    } => {
                        let bytes = match response.bytes().await {
                            Ok(bytes) => bytes,
                            Err(err) => {
                                let message = err.to_string();
                                state_manager.mark_transient_failure(
                                    credential_states,
                                    &provider.channel,
                                    attempt.credential_id,
                                    model.as_deref(),
                                    None,
                                    Some(message.clone()),
                                );
                                return CredentialRetryDecision::Retry {
                                    last_status: None,
                                    last_error: Some(message),
                                    last_request_meta: None,
                                };
                            }
                        };

                        if status_code == 200 {
                            state_manager.mark_success(
                                credential_states,
                                &provider.channel,
                                attempt.credential_id,
                            );
                        }
                        let local = build_model_list_local_response(
                            status_code,
                            &bytes,
                            *page_size,
                            page_token.as_deref(),
                        );
                        CredentialRetryDecision::Return(
                            UpstreamResponse::from_local(local)
                                .with_credential_update(credential_update.clone()),
                        )
                    }
                    AntigravityRequestKind::ModelGet { target } => {
                        let bytes = match response.bytes().await {
                            Ok(bytes) => bytes,
                            Err(err) => {
                                let message = err.to_string();
                                state_manager.mark_transient_failure(
                                    credential_states,
                                    &provider.channel,
                                    attempt.credential_id,
                                    model.as_deref(),
                                    None,
                                    Some(message.clone()),
                                );
                                return CredentialRetryDecision::Retry {
                                    last_status: None,
                                    last_error: Some(message),
                                    last_request_meta: None,
                                };
                            }
                        };

                        if status_code == 200 {
                            state_manager.mark_success(
                                credential_states,
                                &provider.channel,
                                attempt.credential_id,
                            );
                        }
                        let local = build_model_get_local_response(status_code, &bytes, target);
                        CredentialRetryDecision::Return(
                            UpstreamResponse::from_local(local)
                                .with_credential_update(credential_update.clone()),
                        )
                    }
                }
            }
        },
    )
    .await
}

pub async fn execute_antigravity_upstream_usage_with_retry(
    client: &WreqClient,
    provider: &ProviderDefinition,
    credential_states: &ChannelCredentialStateStore,
    credential_id: Option<i64>,
    now_unix_ms: u64,
) -> Result<UpstreamResponse, UpstreamError> {
    let mut scoped_provider = provider.clone();
    if let Some(credential_id) = credential_id {
        scoped_provider
            .credentials
            .credentials
            .retain(|credential| credential.id == credential_id);
    }
    if scoped_provider.credentials.credentials.is_empty() {
        return Err(UpstreamError::NoEligibleCredential {
            channel: scoped_provider.channel.as_str().to_string(),
            model: None,
        });
    }

    let base_url = scoped_provider.settings.base_url().trim();
    if base_url.is_empty() {
        return Err(UpstreamError::InvalidBaseUrl);
    }
    let usage_url = join_base_url_and_path(base_url, "/v1internal:fetchAvailableModels");
    let usage_body = serde_json::to_vec(&json!({}))
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    let state_manager = CredentialStateManager::new(now_unix_ms);
    let usage_url_template = usage_url.clone();
    let channel_id = scoped_provider.channel.clone();
    let user_agent_template = resolve_user_agent_or_default(
        scoped_provider.settings.user_agent(),
        ANTIGRAVITY_USER_AGENT,
    );

    retry_with_eligible_credentials(
        &scoped_provider,
        credential_states,
        None,
        now_unix_ms,
        |credential| {
            if let ChannelCredential::Builtin(BuiltinChannelCredential::Antigravity(value)) =
                &credential.credential
            {
                return antigravity_auth_material_from_credential(value);
            }
            None
        },
        |attempt| {
            let usage_url = usage_url_template.clone();
            let channel_id = channel_id.clone();
            let usage_body = usage_body.clone();
            let user_agent = user_agent_template.clone();
            async move {
                let token_cache_key = format!("{}::{}", channel_id.as_str(), attempt.credential_id);
                let mut credential_update = None;

                let resolved_access_token = match resolve_antigravity_access_token(
                    client,
                    &provider.settings,
                    token_cache_key.as_str(),
                    &attempt.material,
                    now_unix_ms,
                    false,
                )
                .await
                {
                    Ok(token) => token,
                    Err(err) => {
                        let message = err.as_message();
                        if err.is_invalid_credential() {
                            state_manager.mark_auth_dead(
                                credential_states,
                                &channel_id,
                                attempt.credential_id,
                                Some(message.clone()),
                            );
                        } else {
                            state_manager.mark_transient_failure(
                                credential_states,
                                &channel_id,
                                attempt.credential_id,
                                None,
                                None,
                                Some(message.clone()),
                            );
                        }
                        return CredentialRetryDecision::Retry {
                            last_status: None,
                            last_error: Some(message),
                            last_request_meta: None,
                        };
                    }
                };
                if let Some(refreshed) = resolved_access_token.refreshed.as_ref() {
                    credential_update = Some(antigravity_credential_update(
                        attempt.credential_id,
                        refreshed,
                    ));
                }
                let mut request_id = make_request_id();
                let (mut response, mut request_meta) = match send_antigravity_request(
                    client,
                    WreqMethod::POST,
                    usage_url.as_str(),
                    resolved_access_token.access_token.as_str(),
                    user_agent.as_str(),
                    None,
                    None,
                    Some(usage_body.as_slice()),
                    request_id.as_str(),
                )
                .await
                {
                    Ok((response, request_meta)) => (response, request_meta),
                    Err(err) => {
                        let message = err.to_string();
                        state_manager.mark_transient_failure(
                            credential_states,
                            &channel_id,
                            attempt.credential_id,
                            None,
                            None,
                            Some(message.clone()),
                        );
                        return CredentialRetryDecision::Retry {
                            last_status: None,
                            last_error: Some(message),
                            last_request_meta: None,
                        };
                    }
                };

                let mut status_code = response.status().as_u16();
                if is_antigravity_auth_failure(status_code) {
                    let refreshed_access_token = match resolve_antigravity_access_token(
                        client,
                        &provider.settings,
                        token_cache_key.as_str(),
                        &attempt.material,
                        now_unix_ms,
                        true,
                    )
                    .await
                    {
                        Ok(token) => token,
                        Err(err) => {
                            let message = err.as_message();
                            if err.is_invalid_credential() {
                                state_manager.mark_auth_dead(
                                    credential_states,
                                    &channel_id,
                                    attempt.credential_id,
                                    Some(message.clone()),
                                );
                            } else {
                                state_manager.mark_transient_failure(
                                    credential_states,
                                    &channel_id,
                                    attempt.credential_id,
                                    None,
                                    None,
                                    Some(message.clone()),
                                );
                            }
                            return CredentialRetryDecision::Retry {
                                last_status: Some(status_code),
                                last_error: Some(message),
                                last_request_meta: None,
                            };
                        }
                    };
                    if let Some(refreshed) = refreshed_access_token.refreshed.as_ref() {
                        credential_update = Some(antigravity_credential_update(
                            attempt.credential_id,
                            refreshed,
                        ));
                    }
                    request_id = make_request_id();
                    (response, request_meta) = match send_antigravity_request(
                        client,
                        WreqMethod::POST,
                        usage_url.as_str(),
                        refreshed_access_token.access_token.as_str(),
                        user_agent.as_str(),
                        None,
                        None,
                        Some(usage_body.as_slice()),
                        request_id.as_str(),
                    )
                    .await
                    {
                        Ok((response, request_meta)) => (response, request_meta),
                        Err(err) => {
                            let message = err.to_string();
                            state_manager.mark_transient_failure(
                                credential_states,
                                &channel_id,
                                attempt.credential_id,
                                None,
                                None,
                                Some(message.clone()),
                            );
                            return CredentialRetryDecision::Retry {
                                last_status: None,
                                last_error: Some(message),
                                last_request_meta: None,
                            };
                        }
                    };

                    status_code = response.status().as_u16();
                    if is_antigravity_auth_failure(status_code) {
                        let message = format!(
                            "upstream status {} after antigravity access token refresh",
                            status_code
                        );
                        state_manager.mark_auth_dead(
                            credential_states,
                            &channel_id,
                            attempt.credential_id,
                            Some(message.clone()),
                        );
                        return CredentialRetryDecision::Retry {
                            last_status: Some(status_code),
                            last_error: Some(message),
                            last_request_meta: None,
                        };
                    }
                }

                if status_code == 429 {
                    let retry_after_ms = retry_after_to_millis(response.headers());
                    let message = format!("upstream status {status_code}");
                    state_manager.mark_rate_limited(
                        credential_states,
                        &channel_id,
                        attempt.credential_id,
                        None,
                        retry_after_ms,
                        Some(message.clone()),
                    );
                    return CredentialRetryDecision::Retry {
                        last_status: Some(status_code),
                        last_error: Some(message),
                        last_request_meta: None,
                    };
                }

                if is_transient_server_failure(status_code) {
                    let message = format!("upstream status {status_code}");
                    state_manager.mark_transient_failure(
                        credential_states,
                        &channel_id,
                        attempt.credential_id,
                        None,
                        None,
                        Some(message.clone()),
                    );
                    return CredentialRetryDecision::Retry {
                        last_status: Some(status_code),
                        last_error: Some(message),
                        last_request_meta: None,
                    };
                }

                if response.status().is_success() {
                    state_manager.mark_success(
                        credential_states,
                        &channel_id,
                        attempt.credential_id,
                    );
                }
                CredentialRetryDecision::Return(
                    UpstreamResponse::from_http(attempt.credential_id, attempt.attempts, response)
                        .with_request_meta(request_meta)
                        .with_credential_update(credential_update.clone()),
                )
            }
        },
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn send_antigravity_request(
    client: &WreqClient,
    method: WreqMethod,
    url: &str,
    access_token: &str,
    user_agent: &str,
    request_type: Option<&str>,
    session_id: Option<&str>,
    body: Option<&[u8]>,
    request_id: &str,
) -> Result<(WreqResponse, UpstreamRequestMeta), wreq::Error> {
    let mut headers = vec![
        ("accept".to_string(), "application/json".to_string()),
        (
            "authorization".to_string(),
            format!("Bearer {access_token}"),
        ),
        ("user-agent".to_string(), user_agent.to_string()),
        ("accept-encoding".to_string(), "gzip".to_string()),
        ("requestid".to_string(), request_id.to_string()),
    ];
    if let Some(value) = request_type {
        headers.push(("requesttype".to_string(), value.to_string()));
    }
    if let Some(value) = session_id.map(str::trim).filter(|value| !value.is_empty()) {
        headers.push(("x-machine-session-id".to_string(), value.to_string()));
    }
    if body.is_some() {
        headers.push(("content-type".to_string(), "application/json".to_string()));
    }
    crate::channels::upstream::tracked_send_request(
        client,
        method,
        url,
        headers,
        body.map(|value| value.to_vec()),
    )
    .await
}

impl AntigravityPreparedRequest {
    fn from_transform_request(request: &TransformRequest) -> Result<Self, UpstreamError> {
        match request {
            TransformRequest::ModelListGemini(value) => Ok(Self {
                method: WreqMethod::POST,
                path: "/v1internal:fetchAvailableModels".to_string(),
                query: gemini_model_list_query_string(
                    value.query.page_size,
                    value.query.page_token.as_deref(),
                ),
                body: Some(json!({})),
                model: None,
                kind: AntigravityRequestKind::ModelList {
                    page_size: value.query.page_size,
                    page_token: value.query.page_token.clone(),
                },
            }),
            TransformRequest::ModelGetGemini(value) => {
                let target = normalize_model_name(value.path.name.as_str());
                Ok(Self {
                    method: WreqMethod::POST,
                    path: "/v1internal:fetchAvailableModels".to_string(),
                    query: None,
                    body: Some(json!({})),
                    model: Some(normalize_model_id(value.path.name.as_str())),
                    kind: AntigravityRequestKind::ModelGet { target },
                })
            }
            TransformRequest::GenerateContentGemini(value) => {
                let model = normalize_model_id(value.path.model.as_str());
                Ok(Self {
                    method: to_wreq_method(&value.method)?,
                    path: "/v1internal:generateContent".to_string(),
                    query: None,
                    body: Some(
                        serde_json::to_value(&value.body)
                            .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?,
                    ),
                    model: Some(model),
                    kind: AntigravityRequestKind::Forward {
                        requires_project: true,
                        request_type: None,
                    },
                })
            }
            TransformRequest::StreamGenerateContentGeminiSse(value)
            | TransformRequest::StreamGenerateContentGeminiNdjson(value) => {
                let model = normalize_model_id(value.path.model.as_str());
                Ok(Self {
                    method: to_wreq_method(&value.method)?,
                    path: "/v1internal:streamGenerateContent".to_string(),
                    query: Some("alt=sse".to_string()),
                    body: Some(
                        serde_json::to_value(&value.body)
                            .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?,
                    ),
                    model: Some(model),
                    kind: AntigravityRequestKind::Forward {
                        requires_project: true,
                        request_type: None,
                    },
                })
            }
            TransformRequest::EmbeddingGemini(value) => {
                let model = normalize_model_name(value.path.model.as_str());
                Ok(Self {
                    method: to_wreq_method(&value.method)?,
                    path: format!("/v1beta/{model}:embedContent"),
                    query: None,
                    body: Some(
                        serde_json::to_value(&value.body)
                            .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?,
                    ),
                    model: Some(normalize_model_id(value.path.model.as_str())),
                    kind: AntigravityRequestKind::Forward {
                        requires_project: false,
                        request_type: None,
                    },
                })
            }
            _ => Err(UpstreamError::UnsupportedRequest),
        }
    }

    fn from_payload(
        operation: OperationFamily,
        protocol: ProtocolKind,
        body: &[u8],
    ) -> Result<Self, UpstreamError> {
        fn parse_gemini_payload_wrapper(
            payload: &[u8],
        ) -> Result<ParsedGeminiPayload, UpstreamError> {
            let value = serde_json::from_slice::<Value>(payload)
                .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
            let model = value
                .pointer("/path/model")
                .or_else(|| value.pointer("/path/name"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let body_value = value.get("body").cloned();
            let alt = value
                .pointer("/query/alt")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            Ok((model, body_value, alt))
        }

        match (operation, protocol) {
            (OperationFamily::ModelList, ProtocolKind::Gemini) => {
                let payload = serde_json::from_slice::<Value>(body)
                    .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
                let page_size = payload
                    .pointer("/query/page_size")
                    .and_then(Value::as_u64)
                    .map(|value| value as u32);
                let page_token = payload
                    .pointer("/query/page_token")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                Ok(Self {
                    method: WreqMethod::POST,
                    path: "/v1internal:fetchAvailableModels".to_string(),
                    query: gemini_model_list_query_string(page_size, page_token.as_deref()),
                    body: Some(json!({})),
                    model: None,
                    kind: AntigravityRequestKind::ModelList {
                        page_size,
                        page_token,
                    },
                })
            }
            (OperationFamily::ModelGet, ProtocolKind::Gemini) => {
                let payload = serde_json::from_slice::<Value>(body)
                    .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
                let Some(target) = payload
                    .pointer("/path/name")
                    .or_else(|| payload.pointer("/path/model"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                else {
                    return Err(UpstreamError::SerializeRequest(
                        "missing path.name in antigravity model_get payload".to_string(),
                    ));
                };
                Ok(Self {
                    method: WreqMethod::POST,
                    path: "/v1internal:fetchAvailableModels".to_string(),
                    query: None,
                    body: Some(json!({})),
                    model: Some(normalize_model_id(target.as_str())),
                    kind: AntigravityRequestKind::ModelGet {
                        target: normalize_model_name(target.as_str()),
                    },
                })
            }
            (OperationFamily::GenerateContent, ProtocolKind::Gemini) => {
                let (model, body_value, _) = parse_gemini_payload_wrapper(body)?;
                let Some(model) = model else {
                    return Err(UpstreamError::SerializeRequest(
                        "missing path.model in antigravity generate payload".to_string(),
                    ));
                };
                let Some(body_value) = body_value else {
                    return Err(UpstreamError::SerializeRequest(
                        "missing body in antigravity generate payload".to_string(),
                    ));
                };
                Ok(Self {
                    method: WreqMethod::POST,
                    path: "/v1internal:generateContent".to_string(),
                    query: None,
                    body: Some(body_value),
                    model: Some(normalize_model_id(model.as_str())),
                    kind: AntigravityRequestKind::Forward {
                        requires_project: true,
                        request_type: None,
                    },
                })
            }
            (OperationFamily::StreamGenerateContent, ProtocolKind::Gemini)
            | (OperationFamily::StreamGenerateContent, ProtocolKind::GeminiNDJson) => {
                let (model, body_value, alt) = parse_gemini_payload_wrapper(body)?;
                let Some(model) = model else {
                    return Err(UpstreamError::SerializeRequest(
                        "missing path.model in antigravity stream payload".to_string(),
                    ));
                };
                let Some(body_value) = body_value else {
                    return Err(UpstreamError::SerializeRequest(
                        "missing body in antigravity stream payload".to_string(),
                    ));
                };
                Ok(Self {
                    method: WreqMethod::POST,
                    path: "/v1internal:streamGenerateContent".to_string(),
                    query: Some(format!("alt={}", alt.unwrap_or_else(|| "sse".to_string()))),
                    body: Some(body_value),
                    model: Some(normalize_model_id(model.as_str())),
                    kind: AntigravityRequestKind::Forward {
                        requires_project: true,
                        request_type: None,
                    },
                })
            }
            (OperationFamily::Embedding, ProtocolKind::Gemini) => {
                let (model, body_value, _) = parse_gemini_payload_wrapper(body)?;
                let Some(model) = model else {
                    return Err(UpstreamError::SerializeRequest(
                        "missing path.model in antigravity embedding payload".to_string(),
                    ));
                };
                let Some(body_value) = body_value else {
                    return Err(UpstreamError::SerializeRequest(
                        "missing body in antigravity embedding payload".to_string(),
                    ));
                };
                let model_name = normalize_model_name(model.as_str());
                Ok(Self {
                    method: WreqMethod::POST,
                    path: format!("/v1beta/{model_name}:embedContent"),
                    query: None,
                    body: Some(body_value),
                    model: Some(normalize_model_id(model.as_str())),
                    kind: AntigravityRequestKind::Forward {
                        requires_project: false,
                        request_type: None,
                    },
                })
            }
            _ => Err(UpstreamError::UnsupportedRequest),
        }
    }
}

fn build_request_body_bytes(
    body: Option<&Value>,
    model: Option<&str>,
    kind: &AntigravityRequestKind,
    project_id: &str,
    session_id: Option<&str>,
) -> Result<Option<Vec<u8>>, UpstreamError> {
    match kind {
        AntigravityRequestKind::Forward {
            requires_project: true,
            ..
        } => {
            let Some(model) = model else {
                return Err(UpstreamError::SerializeRequest(
                    "missing model for antigravity generate request".to_string(),
                ));
            };
            let project_id = project_id.trim();
            if project_id.is_empty() {
                return Err(UpstreamError::SerializeRequest(
                    "missing project_id in antigravity credential".to_string(),
                ));
            }
            let Some(request) = body else {
                return Err(UpstreamError::SerializeRequest(
                    "missing request body for antigravity generate request".to_string(),
                ));
            };
            let mut request = request.clone();
            if model.to_ascii_lowercase().contains("gemini")
                && let Some(config_obj) = request
                    .as_object_mut()
                    .and_then(|root| root.get_mut("generationConfig"))
                    .and_then(Value::as_object_mut)
            {
                config_obj.remove("logprobs");
                config_obj.remove("responseLogprobs");
                config_obj.remove("response_logprobs");
            }
            if let Some(value) = session_id.map(str::trim).filter(|value| !value.is_empty())
                && let Some(request_obj) = request.as_object_mut()
            {
                request_obj.insert("sessionId".to_string(), Value::String(value.to_string()));
            }
            let wrapped = json!({
                "model": model,
                "project": project_id,
                "request": request,
            });
            Ok(Some(serde_json::to_vec(&wrapped).map_err(|err| {
                UpstreamError::SerializeRequest(err.to_string())
            })?))
        }
        _ => {
            let Some(body) = body else {
                return Ok(None);
            };
            Ok(Some(serde_json::to_vec(body).map_err(|err| {
                UpstreamError::SerializeRequest(err.to_string())
            })?))
        }
    }
}

fn request_type_for_kind(
    kind: &AntigravityRequestKind,
    model: Option<&str>,
) -> Option<&'static str> {
    match kind {
        AntigravityRequestKind::Forward { request_type, .. } => {
            request_type.or_else(|| model.map(request_type_for_model))
        }
        _ => None,
    }
}

fn request_type_for_model(model: &str) -> &'static str {
    if model.to_ascii_lowercase().contains("image") {
        "image_gen"
    } else {
        "agent"
    }
}

fn try_local_antigravity_count_response(
    request: &TransformRequest,
) -> Result<Option<TransformResponse>, UpstreamError> {
    let TransformRequest::CountTokenGemini(value) = request else {
        return Ok(None);
    };

    let payload = serde_json::to_value(&value.body)
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    let text = collect_count_text(&payload);
    let total_tokens = (text.chars().count() as u64).div_ceil(4);

    let response_json = json!({
        "stats_code": 200,
        "headers": {},
        "body": {
            "totalTokens": total_tokens,
        }
    });
    let response = serde_json::from_value(response_json)
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    Ok(Some(TransformResponse::CountTokenGemini(response)))
}

fn collect_count_text(payload: &Value) -> String {
    if let Some(contents) = payload.get("contents").and_then(Value::as_array) {
        return collect_contents_text(contents);
    }
    if let Some(contents) = payload
        .get("generateContentRequest")
        .and_then(|value| value.get("contents"))
        .and_then(Value::as_array)
    {
        return collect_contents_text(contents);
    }
    serde_json::to_string(payload).unwrap_or_default()
}

fn collect_contents_text(contents: &[Value]) -> String {
    let mut out = String::new();
    for content in contents {
        let Some(parts) = content.get("parts").and_then(Value::as_array) else {
            continue;
        };
        for part in parts {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
        }
    }
    out
}

fn build_model_list_local_response(
    status_code: u16,
    bytes: &[u8],
    page_size: Option<u32>,
    page_token: Option<&str>,
) -> TransformResponse {
    if status_code == 200 {
        let payload = serde_json::from_slice::<Value>(bytes).ok();
        if let Some(payload) = payload {
            let models = extract_available_models(&payload);
            let total = models.len();
            let start = page_token
                .and_then(|token| token.parse::<usize>().ok())
                .unwrap_or(0);
            let start = start.min(total);
            let size = page_size
                .map(|value| value.max(1) as usize)
                .unwrap_or(total.saturating_sub(start));
            let end = start.saturating_add(size).min(total);
            let page_models = models[start..end].to_vec();
            let next_page_token = (end < total).then(|| end.to_string());

            let response_json = json!({
                "stats_code": 200,
                "headers": {},
                "body": {
                    "models": page_models,
                    "nextPageToken": next_page_token,
                }
            });
            if let Ok(response) = serde_json::from_value(response_json) {
                return TransformResponse::ModelListGemini(response);
            }
        }
        return model_list_error_response(502, "invalid antigravity model-list payload");
    }

    let message = extract_upstream_error_message(bytes)
        .unwrap_or_else(|| format!("upstream status {status_code}"));
    model_list_error_response(status_code, &message)
}

fn build_model_get_local_response(
    status_code: u16,
    bytes: &[u8],
    target: &str,
) -> TransformResponse {
    if status_code == 200 {
        let payload = serde_json::from_slice::<Value>(bytes).ok();
        if let Some(payload) = payload
            && let Some(model) = find_available_model(&payload, target)
        {
            let response_json = json!({
                "stats_code": 200,
                "headers": {},
                "body": model,
            });
            if let Ok(response) = serde_json::from_value(response_json) {
                return TransformResponse::ModelGetGemini(response);
            }
        }
        let message = format!("model {target} not found");
        return model_get_error_response(404, &message);
    }

    let message = extract_upstream_error_message(bytes)
        .unwrap_or_else(|| format!("upstream status {status_code}"));
    model_get_error_response(status_code, &message)
}

fn model_list_error_response(status_code: u16, message: &str) -> TransformResponse {
    let response_json = json!({
        "stats_code": status_code,
        "headers": {},
        "body": {
            "error": {
                "code": status_code,
                "message": message,
                "status": "UNKNOWN",
            }
        }
    });
    let response = serde_json::from_value(response_json).unwrap_or_else(|_| {
        serde_json::from_value(json!({
            "stats_code": 500,
            "headers": {},
            "body": {
                "error": {
                    "code": 500,
                    "message": "internal serialization error",
                    "status": "INTERNAL",
                }
            }
        }))
        .expect("fallback model-list response")
    });
    TransformResponse::ModelListGemini(response)
}

fn model_get_error_response(status_code: u16, message: &str) -> TransformResponse {
    let response_json = json!({
        "stats_code": status_code,
        "headers": {},
        "body": {
            "error": {
                "code": status_code,
                "message": message,
                "status": "UNKNOWN",
            }
        }
    });
    let response = serde_json::from_value(response_json).unwrap_or_else(|_| {
        serde_json::from_value(json!({
            "stats_code": 500,
            "headers": {},
            "body": {
                "error": {
                    "code": 500,
                    "message": "internal serialization error",
                    "status": "INTERNAL",
                }
            }
        }))
        .expect("fallback model-get response")
    });
    TransformResponse::ModelGetGemini(response)
}

fn extract_available_models(payload: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(models_obj) = payload.get("models").and_then(Value::as_object) {
        for (model_id, model_meta) in models_obj {
            out.push(build_available_model(model_id.as_str(), model_meta));
        }
    } else if let Some(models_arr) = payload.get("models").and_then(Value::as_array) {
        for item in models_arr {
            if let Some(id) = item
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| item.get("name").and_then(Value::as_str))
            {
                out.push(build_available_model(&normalize_model_id(id), item));
            } else if let Some(value) = item.as_str() {
                out.push(build_available_model(
                    &normalize_model_id(value),
                    &Value::Null,
                ));
            }
        }
    }

    out.sort_by(|a, b| {
        let a_name = a.get("name").and_then(Value::as_str).unwrap_or_default();
        let b_name = b.get("name").and_then(Value::as_str).unwrap_or_default();
        a_name.cmp(b_name)
    });
    out.dedup_by(|a, b| {
        let a_name = a.get("name").and_then(Value::as_str).unwrap_or_default();
        let b_name = b.get("name").and_then(Value::as_str).unwrap_or_default();
        a_name == b_name
    });
    out
}

fn find_available_model(payload: &Value, model_name: &str) -> Option<Value> {
    let model_id = normalize_model_id(model_name);
    if let Some(models_obj) = payload.get("models").and_then(Value::as_object) {
        if let Some(meta) = models_obj.get(model_id.as_str()) {
            return Some(build_available_model(model_id.as_str(), meta));
        }
        return models_obj
            .iter()
            .find(|(id, _)| normalize_model_id(id) == model_id)
            .map(|(id, meta)| build_available_model(id.as_str(), meta));
    }

    if let Some(models_arr) = payload.get("models").and_then(Value::as_array) {
        for item in models_arr {
            let raw_id = item
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| item.get("name").and_then(Value::as_str))
                .or_else(|| item.as_str());
            if let Some(raw_id) = raw_id
                && normalize_model_id(raw_id) == model_id
            {
                return Some(build_available_model(model_id.as_str(), item));
            }
        }
    }
    None
}

fn build_available_model(model_id: &str, meta: &Value) -> Value {
    let display_name = meta
        .get("displayName")
        .and_then(Value::as_str)
        .or_else(|| meta.get("display_name").and_then(Value::as_str))
        .unwrap_or(model_id);

    let mut object = Map::new();
    object.insert(
        "name".to_string(),
        Value::String(format!("models/{model_id}")),
    );
    object.insert(
        "baseModelId".to_string(),
        Value::String(model_id.to_string()),
    );
    object.insert("version".to_string(), Value::String("1".to_string()));
    object.insert(
        "displayName".to_string(),
        Value::String(display_name.to_string()),
    );
    object.insert(
        "supportedGenerationMethods".to_string(),
        json!(["generateContent", "countTokens", "streamGenerateContent"]),
    );

    if let Some(limit) = meta.get("maxTokens").and_then(Value::as_u64) {
        object.insert("inputTokenLimit".to_string(), Value::Number(limit.into()));
    }
    if let Some(limit) = meta
        .get("maxOutputTokens")
        .and_then(Value::as_u64)
        .or_else(|| meta.get("outputTokenLimit").and_then(Value::as_u64))
    {
        object.insert("outputTokenLimit".to_string(), Value::Number(limit.into()));
    }

    Value::Object(object)
}

fn extract_upstream_error_message(bytes: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<Value>(bytes).ok()?;
    if let Some(message) = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
    {
        return Some(message.to_string());
    }
    if let Some(message) = value.get("error").and_then(Value::as_str) {
        return Some(message.to_string());
    }
    value
        .get("message")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

const FAKE_PREFIX: &str = "\u{5047}\u{6d41}\u{5f0f}/";
const ANTI_TRUNC_PREFIX: &str = "\u{6d41}\u{5f0f}\u{6297}\u{622a}\u{65ad}/";
const FAKE_SUFFIX: &str = "\u{5047}\u{6d41}\u{5f0f}";
const ANTI_TRUNC_SUFFIX: &str = "\u{6d41}\u{5f0f}\u{6297}\u{622a}\u{65ad}";

fn normalize_model_name(model: &str) -> String {
    let model_id = normalize_model_id(model);
    format!("models/{model_id}")
}

fn normalize_model_id(model: &str) -> String {
    let mut name = model
        .trim()
        .trim_start_matches('/')
        .trim_start_matches("models/");
    for prefix in [FAKE_PREFIX, ANTI_TRUNC_PREFIX] {
        if let Some(stripped) = name.strip_prefix(prefix) {
            name = stripped;
        }
    }
    if let Some(stripped) = name.strip_suffix(FAKE_SUFFIX) {
        name = stripped.trim_end_matches('-');
    }
    if let Some(stripped) = name.strip_suffix(ANTI_TRUNC_SUFFIX) {
        name = stripped.trim_end_matches('-');
    }
    name.to_string()
}

fn make_request_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    format!("gproxy-{nanos}")
}

fn session_id_for_kind(
    kind: &AntigravityRequestKind,
    credential_id: i64,
    project_id: &str,
) -> Option<String> {
    match kind {
        AntigravityRequestKind::Forward {
            requires_project: true,
            ..
        } => Some(stable_session_id(credential_id, project_id)),
        _ => None,
    }
}

fn stable_session_id(credential_id: i64, project_id: &str) -> String {
    let normalized_project = project_id.trim();
    let seed = format!("antigravity:{credential_id}:{normalized_project}");
    let digest = Sha256::digest(seed.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    format!("gproxy-{hex}")
}

fn antigravity_credential_update(
    credential_id: i64,
    refreshed: &AntigravityRefreshedToken,
) -> UpstreamCredentialUpdate {
    UpstreamCredentialUpdate::AntigravityTokenRefresh {
        credential_id,
        access_token: refreshed.access_token.clone(),
        refresh_token: refreshed.refresh_token.clone(),
        expires_at_unix_ms: refreshed.expires_at_unix_ms,
        user_email: refreshed.user_email.clone(),
    }
}

pub fn normalize_antigravity_upstream_response_body(body: &[u8]) -> Option<Vec<u8>> {
    let value = serde_json::from_slice::<Value>(body).ok()?;
    let response = value.get("response")?;
    serde_json::to_vec(response).ok()
}

pub fn normalize_antigravity_upstream_stream_ndjson_chunk(chunk: &[u8]) -> Option<Vec<u8>> {
    normalize_wrapped_response_ndjson_chunk(chunk)
}

fn normalize_wrapped_response_ndjson_chunk(chunk: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(chunk).ok()?;
    let mut out = String::with_capacity(text.len());
    let mut changed = false;

    for segment in text.split_inclusive('\n') {
        let has_newline = segment.ends_with('\n');
        let line = segment.trim_end_matches('\n').trim_end_matches('\r');
        if line.is_empty() {
            out.push_str(segment);
            continue;
        }

        let value = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(_) => {
                out.push_str(segment);
                continue;
            }
        };

        if let Some(response) = value.get("response") {
            let normalized = match serde_json::to_string(response) {
                Ok(value) => value,
                Err(_) => {
                    out.push_str(segment);
                    continue;
                }
            };
            out.push_str(normalized.as_str());
            if has_newline {
                out.push('\n');
            }
            changed = true;
        } else {
            out.push_str(segment);
        }
    }

    changed.then(|| out.into_bytes())
}
