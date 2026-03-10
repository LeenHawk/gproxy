use super::*;

#[derive(Debug, Default, PartialEq, Eq)]
struct TokenErrorDetails {
    error: String,
    description: String,
    code: String,
}

impl TokenErrorDetails {
    fn is_empty(&self) -> bool {
        self.error.is_empty() && self.description.is_empty() && self.code.is_empty()
    }

    fn as_message_suffix(&self) -> String {
        let mut parts = Vec::new();
        if !self.error.is_empty() {
            parts.push(self.error.clone());
        }
        if !self.description.is_empty() {
            parts.push(self.description.clone());
        }
        if !self.code.is_empty() {
            parts.push(format!("code={}", self.code));
        }
        parts.join(" | ")
    }
}

pub(crate) fn codex_auth_material_from_credential(
    value: &CodexCredential,
) -> Option<CodexAuthMaterial> {
    let account_id = value.account_id.trim();
    if account_id.is_empty() {
        return None;
    }

    Some(CodexAuthMaterial {
        access_token: value.access_token.trim().to_string(),
        refresh_token: value.refresh_token.trim().to_string(),
        id_token: value.id_token.trim().to_string(),
        account_id: account_id.to_string(),
        priority_tier: value.priority_tier,
        expires_at_unix_ms: normalize_expires_at_ms(value.expires_at),
    })
}

pub(crate) async fn resolve_codex_access_token(
    client: &WreqClient,
    settings: &ChannelSettings,
    cache_key: &str,
    material: &CodexAuthMaterial,
    now_unix_ms: u64,
    force_refresh: bool,
) -> Result<CodexResolvedAccessToken, CodexTokenRefreshError> {
    if !force_refresh {
        if let Some(cached) = codex_token_cache().get(cache_key).filter(|item| {
            item.expires_at_unix_ms
                .saturating_sub(TOKEN_REFRESH_SKEW_MS)
                > now_unix_ms
        }) {
            return Ok(CodexResolvedAccessToken {
                access_token: cached.access_token.clone(),
                refreshed: None,
            });
        }

        if material.access_token_valid(now_unix_ms) {
            codex_token_cache().insert(
                cache_key.to_string(),
                CachedCodexToken {
                    access_token: material.access_token.clone(),
                    expires_at_unix_ms: material.expires_at_unix_ms,
                },
            );
            return Ok(CodexResolvedAccessToken {
                access_token: material.access_token.clone(),
                refreshed: None,
            });
        }
    }

    let issuer = codex_oauth_issuer_from_settings(settings);
    let refreshed = refresh_access_token(client, issuer.as_str(), material, now_unix_ms).await?;
    codex_token_cache().insert(
        cache_key.to_string(),
        CachedCodexToken {
            access_token: refreshed.access_token.clone(),
            expires_at_unix_ms: refreshed.expires_at_unix_ms,
        },
    );
    Ok(CodexResolvedAccessToken {
        access_token: refreshed.access_token.clone(),
        refreshed: Some(refreshed),
    })
}

pub(super) async fn request_device_user_code(
    client: &WreqClient,
    issuer: &str,
) -> Result<(DeviceUserCodeResponse, UpstreamRequestMeta), UpstreamError> {
    let body = serde_json::to_vec(&json!({ "client_id": CLIENT_ID }))
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    let url = format!(
        "{}/api/accounts/deviceauth/usercode",
        issuer.trim_end_matches('/')
    );
    let headers = vec![("content-type".to_string(), "application/json".to_string())];
    let (response, request_meta) =
        tracked_send_request(client, WreqMethod::POST, url.as_str(), headers, Some(body))
            .await
            .map_err(|err| UpstreamError::UpstreamRequest(err.to_string()))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|err| UpstreamError::UpstreamRequest(err.to_string()))?;
    if !(200..300).contains(&status) {
        let text = String::from_utf8_lossy(&bytes);
        return Err(UpstreamError::UpstreamRequest(format!(
            "deviceauth_usercode_failed: {status} {text}"
        )));
    }
    let parsed = serde_json::from_slice::<DeviceUserCodeResponse>(&bytes)
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    Ok((parsed, request_meta))
}

pub(super) async fn poll_device_authorization(
    client: &WreqClient,
    issuer: &str,
    device_auth_id: &str,
    user_code: &str,
) -> Result<(DeviceAuthPollStatus, UpstreamRequestMeta), UpstreamError> {
    let body = serde_json::to_vec(&json!({
        "device_auth_id": device_auth_id,
        "user_code": user_code,
    }))
    .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    let url = format!(
        "{}/api/accounts/deviceauth/token",
        issuer.trim_end_matches('/')
    );
    let headers = vec![("content-type".to_string(), "application/json".to_string())];
    let (response, request_meta) =
        tracked_send_request(client, WreqMethod::POST, url.as_str(), headers, Some(body))
            .await
            .map_err(|err| UpstreamError::UpstreamRequest(err.to_string()))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|err| UpstreamError::UpstreamRequest(err.to_string()))?;
    if status == 403 || status == 404 {
        return Ok((DeviceAuthPollStatus::Pending, request_meta));
    }
    if !(200..300).contains(&status) {
        let text = String::from_utf8_lossy(&bytes);
        return Err(UpstreamError::UpstreamRequest(format!(
            "deviceauth_poll_failed: {status} {text}"
        )));
    }
    let data = serde_json::from_slice::<DeviceTokenPollResponse>(&bytes)
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    if data.authorization_code.trim().is_empty() || data.code_verifier.trim().is_empty() {
        return Err(UpstreamError::UpstreamRequest(
            "deviceauth_poll_failed: missing authorization_code or code_verifier".to_string(),
        ));
    }
    Ok((DeviceAuthPollStatus::Authorized(data), request_meta))
}

pub(super) async fn exchange_code_for_tokens(
    client: &WreqClient,
    issuer: &str,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
) -> Result<(TokenResponse, UpstreamRequestMeta), UpstreamError> {
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        percent_encode(code),
        percent_encode(redirect_uri),
        percent_encode(CLIENT_ID),
        percent_encode(code_verifier),
    );

    let url = format!("{}/oauth/token", issuer.trim_end_matches('/'));
    let headers = vec![(
        "content-type".to_string(),
        "application/x-www-form-urlencoded".to_string(),
    )];
    let (response, request_meta) = tracked_send_request(
        client,
        WreqMethod::POST,
        url.as_str(),
        headers,
        Some(body.into_bytes()),
    )
    .await
    .map_err(|err| UpstreamError::UpstreamRequest(err.to_string()))?;
    let token = parse_token_response("oauth_token_failed", response).await?;
    Ok((token, request_meta))
}

pub(super) async fn refresh_access_token(
    client: &WreqClient,
    issuer: &str,
    material: &CodexAuthMaterial,
    now_unix_ms: u64,
) -> Result<CodexRefreshedToken, CodexTokenRefreshError> {
    if !material.can_refresh() {
        return Err(CodexTokenRefreshError::InvalidCredential(
            "missing refresh token".to_string(),
        ));
    }

    let body = serde_json::to_vec(&json!({
        "client_id": CLIENT_ID,
        "grant_type": "refresh_token",
        "refresh_token": material.refresh_token,
        "scope": "openid profile email",
    }))
    .map_err(|err| CodexTokenRefreshError::Transient(err.to_string()))?;

    let response = tracked_request(
        client,
        WreqMethod::POST,
        format!("{}/oauth/token", issuer.trim_end_matches('/')).as_str(),
    )
    .header("content-type", "application/json")
    .body(body)
    .send()
    .await
    .map_err(|err| CodexTokenRefreshError::Transient(err.to_string()))?;

    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|err| CodexTokenRefreshError::Transient(err.to_string()))?;
    let parsed = serde_json::from_slice::<TokenResponse>(&bytes).ok();

    if (200..300).contains(&status) {
        let Some(access_token) = parsed
            .as_ref()
            .and_then(|item| item.access_token.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
        else {
            return Err(CodexTokenRefreshError::Transient(
                "oauth token response missing access_token".to_string(),
            ));
        };

        let refresh_token = parsed
            .as_ref()
            .and_then(|item| item.refresh_token.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| material.refresh_token.clone());
        if refresh_token.trim().is_empty() {
            return Err(CodexTokenRefreshError::InvalidCredential(
                "oauth token response missing refresh_token".to_string(),
            ));
        }

        let expires_at_unix_ms = now_unix_ms.saturating_add(
            parsed
                .as_ref()
                .and_then(|item| item.expires_in)
                .unwrap_or(3600)
                .saturating_mul(1000),
        );
        let id_token = parsed
            .as_ref()
            .and_then(|item| item.id_token.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let effective_id_token = id_token
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .or_else(|| (!material.id_token.trim().is_empty()).then(|| material.id_token.clone()));
        let user_email = effective_id_token
            .as_deref()
            .map(parse_id_token_claims)
            .and_then(|claims| claims.email);
        return Ok(CodexRefreshedToken {
            access_token,
            refresh_token,
            expires_at_unix_ms,
            user_email,
            id_token,
        });
    }

    let payload_text = String::from_utf8_lossy(&bytes).to_string();
    let details = extract_token_error_details(parsed.as_ref());
    let message = if details.is_empty() {
        format!("refresh_token_failed: status {status}: {payload_text}")
    } else {
        format!(
            "refresh_token_failed: status {status}: {}",
            details.as_message_suffix()
        )
    };

    if is_invalid_oauth_credential_failure(
        status,
        details.error.as_str(),
        details.description.as_str(),
        details.code.as_str(),
    ) {
        Err(CodexTokenRefreshError::InvalidCredential(message))
    } else {
        Err(CodexTokenRefreshError::Transient(message))
    }
}

pub(super) async fn parse_token_response(
    error_prefix: &str,
    response: wreq::Response,
) -> Result<TokenResponse, UpstreamError> {
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|err| UpstreamError::UpstreamRequest(err.to_string()))?;
    if !(200..300).contains(&status) {
        let text = String::from_utf8_lossy(&bytes);
        return Err(UpstreamError::UpstreamRequest(format!(
            "{error_prefix}: {status} {text}"
        )));
    }
    serde_json::from_slice::<TokenResponse>(&bytes)
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))
}

pub(super) fn is_invalid_oauth_credential_failure(
    status: u16,
    error: &str,
    description: &str,
    code: &str,
) -> bool {
    if !matches!(status, 400 | 401 | 403) {
        return false;
    }
    let joined = format!(
        "{} {} {}",
        error.to_ascii_lowercase(),
        description.to_ascii_lowercase(),
        code.to_ascii_lowercase(),
    );
    joined.contains("invalid_grant")
        || joined.contains("invalid_client")
        || joined.contains("unauthorized_client")
        || joined.contains("invalid_scope")
        || joined.contains("refresh_token_expired")
        || joined.contains("refresh_token_reused")
        || joined.contains("refresh_token_invalidated")
}

fn extract_token_error_details(parsed: Option<&TokenResponse>) -> TokenErrorDetails {
    let Some(parsed) = parsed else {
        return TokenErrorDetails::default();
    };

    let top_level_description = parsed
        .error_description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    match parsed.error.as_ref() {
        Some(Value::String(error)) => TokenErrorDetails {
            error: error.trim().to_string(),
            description: top_level_description.unwrap_or_default(),
            code: String::new(),
        },
        Some(Value::Object(error)) => {
            let error_type = error
                .get("type")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_default();
            let code = error
                .get("code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_default();
            let nested_message = error
                .get("message")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);

            TokenErrorDetails {
                error: error_type,
                description: top_level_description.or(nested_message).unwrap_or_default(),
                code,
            }
        }
        Some(other) => TokenErrorDetails {
            error: String::new(),
            description: top_level_description.unwrap_or_else(|| other.to_string()),
            code: String::new(),
        },
        None => TokenErrorDetails {
            error: String::new(),
            description: top_level_description.unwrap_or_default(),
            code: String::new(),
        },
    }
}

pub(super) fn parse_id_token_claims(id_token: &str) -> IdTokenClaims {
    let mut claims = IdTokenClaims::default();
    let mut parts = id_token.split('.');
    let (_header, payload_b64, _signature) = match (parts.next(), parts.next(), parts.next()) {
        (Some(header), Some(payload), Some(signature))
            if !header.is_empty() && !payload.is_empty() && !signature.is_empty() =>
        {
            (header, payload, signature)
        }
        _ => return claims,
    };

    let payload_bytes = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload_b64) {
        Ok(bytes) => bytes,
        Err(_) => return claims,
    };
    let payload = match serde_json::from_slice::<Value>(&payload_bytes) {
        Ok(value) => value,
        Err(_) => return claims,
    };

    let email = payload
        .get("email")
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .get("https://api.openai.com/profile")
                .and_then(|profile| profile.get("email"))
                .and_then(Value::as_str)
        })
        .map(ToString::to_string);

    let (plan, account_id) = payload
        .get("https://api.openai.com/auth")
        .map(|auth| {
            let plan = auth
                .get("chatgpt_plan_type")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            let account_id = auth
                .get("chatgpt_account_id")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            (plan, account_id)
        })
        .unwrap_or((None, None));

    claims.email = email;
    claims.plan = plan;
    claims.account_id = account_id;
    claims
}

pub(super) fn normalize_expires_at_ms(value: i64) -> u64 {
    if value <= 0 {
        return 0;
    }
    value as u64
}

pub(super) fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub(super) fn default_poll_interval_secs() -> u64 {
    5
}

pub(super) fn percent_encode(value: &str) -> String {
    form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::{TokenResponse, extract_token_error_details, is_invalid_oauth_credential_failure};
    use serde_json::json;

    #[test]
    fn nested_refresh_token_reused_error_is_detected_as_invalid_credential() {
        let parsed = TokenResponse {
            access_token: None,
            refresh_token: None,
            id_token: None,
            expires_in: None,
            error: Some(json!({
                "message": "Your refresh token has already been used to generate a new access token. Please try signing in again.",
                "type": "invalid_request_error",
                "param": null,
                "code": "refresh_token_reused"
            })),
            error_description: None,
        };

        let details = extract_token_error_details(Some(&parsed));
        assert_eq!(details.error, "invalid_request_error");
        assert_eq!(details.code, "refresh_token_reused");
        assert!(details.description.contains("already been used"));
        assert!(is_invalid_oauth_credential_failure(
            401,
            details.error.as_str(),
            details.description.as_str(),
            details.code.as_str(),
        ));
    }

    #[test]
    fn string_error_and_description_still_detect_invalid_credential() {
        let parsed = TokenResponse {
            access_token: None,
            refresh_token: None,
            id_token: None,
            expires_in: None,
            error: Some(json!("invalid_grant")),
            error_description: Some("refresh token expired".to_string()),
        };

        let details = extract_token_error_details(Some(&parsed));
        assert_eq!(details.error, "invalid_grant");
        assert_eq!(details.description, "refresh token expired");
        assert!(is_invalid_oauth_credential_failure(
            400,
            details.error.as_str(),
            details.description.as_str(),
            details.code.as_str(),
        ));
    }
}
