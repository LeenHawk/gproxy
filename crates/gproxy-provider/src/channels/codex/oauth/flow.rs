use super::*;

pub(super) fn parse_oauth_mode(value: Option<&str>) -> OAuthMode {
    let Some(raw) = value else {
        return OAuthMode::DeviceAuth;
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "authorization_code" | "auth_code" | "pkce" | "browser" | "browser_auth" => {
            OAuthMode::AuthorizationCode
        }
        _ => OAuthMode::DeviceAuth,
    }
}

pub(super) fn generate_oauth_state() -> String {
    let mut state_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut state_bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(state_bytes)
}

pub(super) fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub(super) fn generate_code_challenge(code_verifier: &str) -> String {
    let digest = sha2::Sha256::digest(code_verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

pub(super) fn build_authorize_url(
    issuer: &str,
    redirect_uri: &str,
    scope: &str,
    originator: &str,
    code_challenge: &str,
    state: &str,
    allowed_workspace_id: Option<&str>,
) -> String {
    let mut query = vec![
        ("response_type".to_string(), "code".to_string()),
        ("client_id".to_string(), CLIENT_ID.to_string()),
        ("redirect_uri".to_string(), redirect_uri.to_string()),
        ("scope".to_string(), scope.to_string()),
        ("code_challenge".to_string(), code_challenge.to_string()),
        ("code_challenge_method".to_string(), "S256".to_string()),
        ("id_token_add_organizations".to_string(), "true".to_string()),
        ("codex_cli_simplified_flow".to_string(), "true".to_string()),
        ("state".to_string(), state.to_string()),
        ("originator".to_string(), originator.to_string()),
    ];
    if let Some(workspace_id) = allowed_workspace_id
        && !workspace_id.trim().is_empty()
    {
        query.push(("allowed_workspace_id".to_string(), workspace_id.to_string()));
    }
    let qs = query
        .into_iter()
        .map(|(key, value)| {
            format!(
                "{key}={}",
                form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{}/oauth/authorize?{qs}", issuer.trim_end_matches('/'))
}

pub(super) fn resolve_manual_code_and_state(
    query: Option<&str>,
) -> Result<(String, Option<String>), &'static str> {
    let mut code = parse_query_value(query, "code");
    let mut state = parse_query_value(query, "state");
    if let Some(callback_url) = parse_query_value(query, "callback_url") {
        let (code_from_callback, state_from_callback) =
            extract_code_state_from_callback_url(callback_url.as_str());
        if code.is_none() {
            code = code_from_callback;
        }
        if state.is_none() {
            state = state_from_callback;
        }
    }

    let Some(code) = code.filter(|value| !value.trim().is_empty()) else {
        return Err("missing code");
    };
    Ok((code, state))
}

pub(super) fn extract_code_state_from_callback_url(
    callback_url: &str,
) -> (Option<String>, Option<String>) {
    let raw = callback_url.trim();
    if raw.is_empty() {
        return (None, None);
    }

    let query = if let Some((_, query)) = raw.split_once('?') {
        query
    } else {
        raw
    };
    (
        parse_query_value(Some(query), "code"),
        parse_query_value(Some(query), "state"),
    )
}

pub(super) fn parse_query_value(query: Option<&str>, key: &str) -> Option<String> {
    let raw = query?.trim().trim_start_matches('?');
    for (name, value) in form_urlencoded::parse(raw.as_bytes()) {
        if name == key {
            return Some(value.into_owned());
        }
    }
    None
}

pub(super) fn codex_oauth_issuer(settings: &ChannelSettings, query: Option<&str>) -> String {
    parse_query_value(query, "oauth_issuer")
        .or_else(|| parse_query_value(query, "issuer"))
        .or_else(|| {
            settings
                .oauth_issuer_url()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| DEFAULT_ISSUER.to_string())
}

pub(super) fn codex_oauth_issuer_from_settings(settings: &ChannelSettings) -> String {
    settings
        .oauth_issuer_url()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ISSUER)
        .to_string()
}

pub(super) fn prune_oauth_states(now_unix_ms: u64) {
    let states = oauth_states();
    let expired = states
        .iter()
        .filter_map(|entry| {
            let created_at_unix_ms = match entry.value() {
                OAuthState::DeviceAuth {
                    created_at_unix_ms, ..
                }
                | OAuthState::AuthorizationCode {
                    created_at_unix_ms, ..
                } => *created_at_unix_ms,
            };
            if now_unix_ms.saturating_sub(created_at_unix_ms) > OAUTH_STATE_TTL_MS {
                Some(entry.key().clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for key in expired {
        states.remove(key.as_str());
    }
}

pub(super) fn json_oauth_response(status_code: u16, body: Value) -> UpstreamOAuthResponse {
    json_oauth_response_with_meta(status_code, body, None)
}

pub(super) fn json_oauth_response_with_meta(
    status_code: u16,
    body: Value,
    request_meta: Option<UpstreamRequestMeta>,
) -> UpstreamOAuthResponse {
    UpstreamOAuthResponse {
        status_code,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: serde_json::to_vec(&body).unwrap_or_default(),
        request_meta,
    }
}

pub(super) fn json_oauth_error(status_code: u16, message: &str) -> UpstreamOAuthResponse {
    json_oauth_error_with_meta(status_code, message, None)
}

pub(super) fn json_oauth_error_with_meta(
    status_code: u16,
    message: &str,
    request_meta: Option<UpstreamRequestMeta>,
) -> UpstreamOAuthResponse {
    json_oauth_response_with_meta(status_code, json!({ "error": message }), request_meta)
}

pub(super) fn build_callback_result(
    tokens: TokenResponse,
    request_meta: Option<UpstreamRequestMeta>,
) -> Result<UpstreamOAuthCallbackResult, UpstreamError> {
    let Some(access_token) = tokens
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
    else {
        return Ok(UpstreamOAuthCallbackResult {
            response: json_oauth_error_with_meta(400, "missing_access_token", request_meta.clone()),
            credential: None,
        });
    };

    let Some(refresh_token) = tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
    else {
        return Ok(UpstreamOAuthCallbackResult {
            response: json_oauth_error_with_meta(
                400,
                "missing_refresh_token",
                request_meta.clone(),
            ),
            credential: None,
        });
    };

    let Some(id_token) = tokens
        .id_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
    else {
        return Ok(UpstreamOAuthCallbackResult {
            response: json_oauth_error_with_meta(400, "missing_id_token", request_meta.clone()),
            credential: None,
        });
    };

    let claims = parse_id_token_claims(id_token.as_str());
    let Some(account_id) = claims.account_id.clone() else {
        return Ok(UpstreamOAuthCallbackResult {
            response: json_oauth_error_with_meta(400, "missing_account_id", request_meta.clone()),
            credential: None,
        });
    };

    let expires_at_unix_ms =
        current_unix_ms().saturating_add(tokens.expires_in.unwrap_or(3600).saturating_mul(1000));

    let credential = UpstreamOAuthCredential {
        label: claims
            .email
            .clone()
            .or_else(|| Some(format!("codex:{account_id}"))),
        credential: ChannelCredential::Builtin(BuiltinChannelCredential::Codex(CodexCredential {
            access_token: access_token.clone(),
            refresh_token: refresh_token.clone(),
            id_token: id_token.clone(),
            priority_tier: None,
            user_email: claims.email.clone(),
            account_id: account_id.clone(),
            expires_at: expires_at_unix_ms.min(i64::MAX as u64) as i64,
        })),
    };

    Ok(UpstreamOAuthCallbackResult {
        response: json_oauth_response_with_meta(
            200,
            json!({
                "access_token": access_token,
                "refresh_token": refresh_token,
                "id_token": id_token,
                "account_id": account_id,
                "email": claims.email,
                "plan": claims.plan,
                "expires_at_unix_ms": expires_at_unix_ms,
            }),
            request_meta,
        ),
        credential: Some(credential),
    })
}
