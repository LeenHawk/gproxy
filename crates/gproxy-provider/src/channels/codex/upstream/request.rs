use super::*;

fn response_header_value(response: &wreq::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase())
}

fn is_probable_html_edge_block(response: &wreq::Response) -> bool {
    if response.status().as_u16() != 403 {
        return false;
    }

    let content_type = response_header_value(response, "content-type").unwrap_or_default();
    let server = response_header_value(response, "server").unwrap_or_default();
    let is_html =
        content_type.contains("text/html") || content_type.contains("application/xhtml+xml");
    let is_cloudflare = server.contains("cloudflare")
        || response.headers().contains_key("cf-ray")
        || response.headers().contains_key("cf-cache-status");

    is_html || (is_cloudflare && !content_type.contains("application/json"))
}

fn codex_html_edge_block_retry(
    state_manager: &CredentialStateManager,
    credential_states: &ChannelCredentialStateStore,
    provider: &ProviderDefinition,
    credential_id: i64,
    model: Option<&str>,
    status_code: u16,
) -> CredentialRetryDecision<UpstreamResponse> {
    let message = format!("upstream status {status_code} (probable html edge block)");
    state_manager.mark_transient_failure(
        credential_states,
        &provider.channel,
        credential_id,
        model,
        None,
        Some(message.clone()),
    );
    CredentialRetryDecision::Retry {
        last_status: Some(status_code),
        last_error: Some(message),
        last_request_meta: None,
    }
}

pub(super) async fn execute_codex_with_prepared(
    client: &WreqClient,
    provider: &ProviderDefinition,
    credential_states: &ChannelCredentialStateStore,
    prepared: CodexPreparedRequest,
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
    let body_template = prepared.body.clone();
    let model_template = prepared.model.clone();
    let kind_template = prepared.kind.clone();
    let extra_headers_template = prepared.extra_headers.clone();
    let base_url_template = base_url.to_string();
    let user_agent_template =
        resolve_user_agent_or_default(provider.settings.user_agent(), USER_AGENT_VALUE);
    let pick_mode =
        credential_pick_mode(provider.credential_pick_mode, cache_affinity_hint.as_ref());

    retry_with_eligible_credentials_with_affinity(
        crate::channels::retry::CredentialRetryContext {
            provider,
            credential_states,
            model: prepared.model.as_deref(),
            now_unix_ms,
            pick_mode,
            cache_affinity_hint,
        },
        |credential| {
            if let ChannelCredential::Builtin(BuiltinChannelCredential::Codex(value)) =
                &credential.credential
            {
                return codex_auth_material_from_credential(value);
            }
            None
        },
        |attempt| {
            let method = method_template.clone();
            let path = path_template.clone();
            let body_template = body_template.clone();
            let model = model_template.clone();
            let kind = kind_template.clone();
            let extra_headers = extra_headers_template.clone();
            let base_url = base_url_template.clone();
            let user_agent = user_agent_template.clone();

            async move {
                let url = join_base_url_and_path(base_url.as_str(), path.as_str());
                let body = apply_codex_priority_tier_override(
                    body_template.as_deref(),
                    attempt.material.priority_tier,
                );
                let token_cache_key =
                    format!("{}::{}", provider.channel.as_str(), attempt.credential_id);
                let mut credential_update = None;

                let access_token = match resolve_codex_access_token(
                    client,
                    &provider.settings,
                    token_cache_key.as_str(),
                    &attempt.material,
                    now_unix_ms,
                    false,
                )
                .await
                {
                    Ok(token) => {
                        if let Some(refreshed) = token.refreshed.as_ref() {
                            credential_update =
                                Some(codex_credential_update(attempt.credential_id, refreshed));
                        }
                        token.access_token
                    }
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
                let (mut response, mut request_meta) = match send_codex_request(
                    client,
                    CodexRequestParams {
                        method: method.clone(),
                        url: url.as_str(),
                        access_token: access_token.as_str(),
                        account_id: attempt.material.account_id.as_str(),
                        user_agent: user_agent.as_str(),
                        extra_headers: extra_headers.as_slice(),
                        body: body.as_deref(),
                    },
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
                if is_probable_html_edge_block(&response) {
                    return codex_html_edge_block_retry(
                        &state_manager,
                        credential_states,
                        provider,
                        attempt.credential_id,
                        model.as_deref(),
                        status_code,
                    );
                }
                if is_auth_failure(status_code) {
                    let refreshed_token = match resolve_codex_access_token(
                        client,
                        &provider.settings,
                        token_cache_key.as_str(),
                        &attempt.material,
                        now_unix_ms,
                        true,
                    )
                    .await
                    {
                        Ok(token) => {
                            if let Some(refreshed) = token.refreshed.as_ref() {
                                credential_update =
                                    Some(codex_credential_update(attempt.credential_id, refreshed));
                            }
                            token.access_token
                        }
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
                    (response, request_meta) = match send_codex_request(
                        client,
                        CodexRequestParams {
                            method,
                            url: url.as_str(),
                            access_token: refreshed_token.as_str(),
                            account_id: attempt.material.account_id.as_str(),
                            user_agent: user_agent.as_str(),
                            extra_headers: extra_headers.as_slice(),
                            body: body.as_deref(),
                        },
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
                    if is_probable_html_edge_block(&response) {
                        return codex_html_edge_block_retry(
                            &state_manager,
                            credential_states,
                            provider,
                            attempt.credential_id,
                            model.as_deref(),
                            status_code,
                        );
                    }
                    if is_auth_failure(status_code) {
                        let message = format!(
                            "upstream status {} after codex access token refresh",
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

                match kind {
                    CodexRequestKind::Forward => {
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
                            .with_request_meta(request_meta.clone())
                            .with_credential_update(credential_update.clone()),
                        )
                    }
                    CodexRequestKind::ModelList => {
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

                        let local = build_model_list_local_response(status_code, &bytes);
                        if status_code == 200 {
                            state_manager.mark_success(
                                credential_states,
                                &provider.channel,
                                attempt.credential_id,
                            );
                        }
                        CredentialRetryDecision::Return(
                            UpstreamResponse::from_local(local)
                                .with_request_meta(request_meta.clone())
                                .with_credential_update(credential_update.clone()),
                        )
                    }
                    CodexRequestKind::ModelGet { target } => {
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

                        let local = build_model_get_local_response(status_code, &bytes, &target);
                        if status_code == 200 {
                            state_manager.mark_success(
                                credential_states,
                                &provider.channel,
                                attempt.credential_id,
                            );
                        }
                        CredentialRetryDecision::Return(
                            UpstreamResponse::from_local(local)
                                .with_request_meta(request_meta.clone())
                                .with_credential_update(credential_update.clone()),
                        )
                    }
                }
            }
        },
    )
    .await
}
