use base64::Engine as _;
use gproxy_channel_api::{
    BoxFuture, CallerIdentity, OAuthAuthorizeInput, OAuthBrowserUser, OAuthCodeGrant,
    OAuthDevicePoll, OAuthDeviceStart, OAuthError, OAuthService, OAuthTokenSet,
};
use gproxy_store::records::{
    OAuthCodeInput, OAuthDeviceInput, OAuthGrantInput, OAuthGrantRecord, OAuthTokenInput,
};
use sha2::{Digest, Sha256};

use super::AppHost;

const PORTAL_COOKIE: &str = "gproxy_portal_session";
const ACCESS_SECONDS: i64 = 60 * 60;
const REFRESH_SECONDS: i64 = 30 * 24 * 60 * 60;
const CODE_SECONDS: i64 = 5 * 60;
const DEVICE_SECONDS: i64 = 15 * 60;

impl OAuthService for AppHost {
    fn browser_user<'a>(
        &'a self,
        headers: &'a http::HeaderMap,
    ) -> BoxFuture<'a, Result<Option<OAuthBrowserUser>, OAuthError>> {
        Box::pin(async move {
            let Some(token) = cookie(headers, PORTAL_COOKIE) else {
                return Ok(None);
            };
            let Some(user) = self
                .services
                .store
                .user_for_session(&digest(token), now())
                .await
                .map_err(store)?
            else {
                return Ok(None);
            };
            Ok(Some(OAuthBrowserUser {
                identity: CallerIdentity {
                    user_id: user.id,
                    user_key_id: 0,
                    org_id: user.organization_id,
                    team_id: user.team_id,
                },
                name: user.name,
            }))
        })
    }

    fn authorize<'a>(
        &'a self,
        user: &'a OAuthBrowserUser,
        input: OAuthAuthorizeInput,
    ) -> BoxFuture<'a, Result<OAuthCodeGrant, OAuthError>> {
        Box::pin(async move { create_grant_code(self, user, input, now()).await })
    }

    fn exchange_code<'a>(
        &'a self,
        code: &'a str,
        client_id: &'a str,
        redirect_uri: &'a str,
        verifier: &'a str,
        issuer: &'a str,
    ) -> BoxFuture<'a, Result<OAuthTokenSet, OAuthError>> {
        Box::pin(async move {
            let current = now();
            let record = self
                .services
                .store
                .oauth_code(&digest(code))
                .await
                .map_err(store)?
                .ok_or(OAuthError::InvalidGrant)?;
            if record.consumed_at.is_some()
                || record.expires_at <= current
                || record.grant.revoked_at.is_some()
                || record.grant.client_id != client_id
                || record.redirect_uri != redirect_uri
                || pkce(verifier) != record.code_challenge
                || !self
                    .services
                    .store
                    .consume_oauth_code(record.id, current)
                    .await
                    .map_err(store)?
            {
                return Err(OAuthError::InvalidGrant);
            }
            issue(self, &record.grant, issuer, current).await
        })
    }

    fn refresh<'a>(
        &'a self,
        refresh_token: &'a str,
        client_id: &'a str,
        issuer: &'a str,
    ) -> BoxFuture<'a, Result<OAuthTokenSet, OAuthError>> {
        Box::pin(async move {
            let current = now();
            let record = self
                .services
                .store
                .oauth_token(&digest(refresh_token))
                .await
                .map_err(store)?
                .ok_or(OAuthError::InvalidGrant)?;
            if record.kind != "refresh"
                || record.consumed_at.is_some()
                || record.revoked_at.is_some()
                || record.expires_at <= current
                || record.grant.revoked_at.is_some()
                || record.grant.client_id != client_id
                || !self
                    .services
                    .store
                    .consume_oauth_token(record.id, current)
                    .await
                    .map_err(store)?
            {
                return Err(OAuthError::InvalidGrant);
            }
            issue(self, &record.grant, issuer, current).await
        })
    }

    fn revoke<'a>(&'a self, token: &'a str) -> BoxFuture<'a, Result<(), OAuthError>> {
        Box::pin(async move {
            if let Some(record) = self
                .services
                .store
                .oauth_token(&digest(token))
                .await
                .map_err(store)?
            {
                self.services
                    .store
                    .revoke_oauth_grant(record.grant.id, record.grant.user_key_id, now())
                    .await
                    .map_err(store)?;
                self.services.control.reload().await.map_err(store)?;
            }
            Ok(())
        })
    }

    fn device_start<'a>(
        &'a self,
        provider_id: i64,
        client_id: &'a str,
        _issuer: &'a str,
    ) -> BoxFuture<'a, Result<OAuthDeviceStart, OAuthError>> {
        Box::pin(async move {
            let current = now();
            let device_auth_id = random_url(32)?;
            let user_code = user_code()?;
            self.services
                .store
                .insert_oauth_device(&OAuthDeviceInput {
                    device_digest: digest(&device_auth_id),
                    user_code: user_code.clone(),
                    client_id: client_id.into(),
                    provider_id,
                    created_at: current,
                    expires_at: current + DEVICE_SECONDS,
                })
                .await
                .map_err(store)?;
            Ok(OAuthDeviceStart {
                device_auth_id,
                user_code,
                interval_secs: 5,
            })
        })
    }

    fn device_poll<'a>(
        &'a self,
        device_auth_id: &'a str,
        user_code: &'a str,
    ) -> BoxFuture<'a, Result<OAuthDevicePoll, OAuthError>> {
        Box::pin(async move {
            let record = self
                .services
                .store
                .oauth_device_by_digest(&digest(device_auth_id))
                .await
                .map_err(store)?
                .filter(|record| record.user_code == normalize_code(user_code))
                .ok_or(OAuthError::AccessDenied)?;
            if record.expires_at <= now() {
                return Ok(OAuthDevicePoll::Denied);
            }
            let Some(envelope) = record.envelope else {
                return Ok(OAuthDevicePoll::Pending);
            };
            let value = self
                .services
                .cipher
                .open_user_key(&envelope)
                .map_err(|error| OAuthError::Store(error.to_string()))?;
            Ok(OAuthDevicePoll::Ready {
                authorization_code: field(&value, "authorization_code")?.into(),
                code_verifier: field(&value, "code_verifier")?.into(),
                code_challenge: field(&value, "code_challenge")?.into(),
            })
        })
    }

    fn device_approve<'a>(
        &'a self,
        user: &'a OAuthBrowserUser,
        user_code: &'a str,
        issuer: &'a str,
    ) -> BoxFuture<'a, Result<(), OAuthError>> {
        Box::pin(async move {
            let current = now();
            let record = self
                .services
                .store
                .oauth_device_by_code(&normalize_code(user_code))
                .await
                .map_err(store)?
                .filter(|record| record.expires_at > current && record.approved_at.is_none())
                .ok_or(OAuthError::InvalidRequest)?;
            let verifier = random_url(48)?;
            let challenge = pkce(&verifier);
            let granted = create_grant_code(
                self,
                user,
                OAuthAuthorizeInput {
                    provider_id: record.provider_id,
                    client_id: record.client_id,
                    redirect_uri: format!("{}/deviceauth/callback", issuer.trim_end_matches('/')),
                    scopes: vec![
                        "openid".into(),
                        "profile".into(),
                        "email".into(),
                        "offline_access".into(),
                        "api.connectors.read".into(),
                        "api.connectors.invoke".into(),
                    ],
                    code_challenge: challenge.clone(),
                },
                current,
            )
            .await?;
            let code_record = self
                .services
                .store
                .oauth_code(&digest(&granted.code))
                .await
                .map_err(store)?
                .ok_or(OAuthError::TemporarilyUnavailable)?;
            let envelope = self
                .services
                .cipher
                .seal_user_key(&serde_json::json!({
                    "authorization_code":granted.code,
                    "code_verifier":verifier,
                    "code_challenge":challenge
                }))
                .map_err(|error| OAuthError::Store(error.to_string()))?;
            if !self
                .services
                .store
                .approve_oauth_device(record.id, code_record.grant.id, &envelope, current)
                .await
                .map_err(store)?
            {
                return Err(OAuthError::InvalidRequest);
            }
            Ok(())
        })
    }
}

async fn create_grant_code(
    host: &AppHost,
    user: &OAuthBrowserUser,
    input: OAuthAuthorizeInput,
    current: i64,
) -> Result<OAuthCodeGrant, OAuthError> {
    let api_key = format!("at-gp-oauth-{}", random_url(32)?);
    let digest_version = crate::control::USER_KEY_DIGEST_VERSION;
    let key_digest = crate::control::user_key_digest(digest_version, &api_key)
        .ok_or(OAuthError::TemporarilyUnavailable)?;
    let envelope = host
        .services
        .cipher
        .seal_user_key(&serde_json::Value::String(api_key.clone()))
        .map_err(|error| OAuthError::Store(error.to_string()))?;
    let user_key_id = host
        .services
        .store
        .insert_user_key(&gproxy_store::records::UserKeyInput {
            user_id: user.identity.user_id,
            digest: key_digest,
            digest_version,
            prefix: api_key.chars().take(12).collect(),
            envelope,
            label: Some("Codex OAuth".into()),
            expires_at: None,
            enabled: true,
        })
        .await
        .map_err(store)?;
    let grant_id = host
        .services
        .store
        .insert_oauth_grant(&OAuthGrantInput {
            user_id: user.identity.user_id,
            user_key_id,
            provider_id: input.provider_id,
            client_id: input.client_id,
            scopes: input.scopes.join(" "),
            chatgpt_user_id: stable_id("user", input.provider_id, user.identity.user_id),
            chatgpt_account_id: stable_id("account", input.provider_id, user.identity.user_id),
            created_at: current,
        })
        .await
        .map_err(store)?;
    let code = random_url(32)?;
    host.services
        .store
        .insert_oauth_code(&OAuthCodeInput {
            digest: digest(&code),
            grant_id,
            redirect_uri: input.redirect_uri,
            code_challenge: input.code_challenge,
            created_at: current,
            expires_at: current + CODE_SECONDS,
        })
        .await
        .map_err(store)?;
    host.services.control.reload().await.map_err(store)?;
    Ok(OAuthCodeGrant { code })
}

async fn issue(
    host: &AppHost,
    grant: &OAuthGrantRecord,
    issuer: &str,
    current: i64,
) -> Result<OAuthTokenSet, OAuthError> {
    let access = jwt(host, grant, issuer, current, current + ACCESS_SECONDS)?;
    let refresh = format!("rt-gp-{}", random_url(48)?);
    for (token, kind, expires_at) in [
        (&access, "access", current + ACCESS_SECONDS),
        (&refresh, "refresh", current + REFRESH_SECONDS),
    ] {
        host.services
            .store
            .insert_oauth_token(&OAuthTokenInput {
                digest: digest(token),
                grant_id: grant.id,
                kind: kind.into(),
                created_at: current,
                expires_at,
            })
            .await
            .map_err(store_error)?;
    }
    Ok(OAuthTokenSet {
        id_token: jwt(host, grant, issuer, current, current + ACCESS_SECONDS)?,
        access_token: access,
        refresh_token: refresh,
        expires_in: ACCESS_SECONDS as u64,
    })
}

fn jwt(
    host: &AppHost,
    grant: &OAuthGrantRecord,
    issuer: &str,
    issued_at: i64,
    expires_at: i64,
) -> Result<String, OAuthError> {
    let header = encode_json(&serde_json::json!({"alg":"HS256","typ":"JWT"}))?;
    let payload = encode_json(&serde_json::json!({
        "iss": issuer,
        "aud": grant.client_id,
        "sub": grant.chatgpt_user_id,
        "iat": issued_at,
        "exp": expires_at,
        "jti": random_url(18)?,
        "https://api.openai.com/auth": {
            "chatgpt_plan_type": "pro",
            "chatgpt_account_id": grant.chatgpt_account_id,
            "chatgpt_user_id": grant.chatgpt_user_id,
            "chatgpt_account_is_fedramp": false,
            "completed_platform_onboarding": true,
            "is_org_owner": false
        }
    }))?;
    let signing_input = format!("{header}.{payload}");
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(host.services.cipher.sign_oauth(signing_input.as_bytes()));
    Ok(format!("{signing_input}.{signature}"))
}

fn encode_json(value: &serde_json::Value) -> Result<String, OAuthError> {
    serde_json::to_vec(value)
        .map(|bytes| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
        .map_err(|error| OAuthError::Store(error.to_string()))
}

fn pkce(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn digest(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}

fn stable_id(kind: &str, provider_id: i64, user_id: i64) -> String {
    let digest = Sha256::digest(format!("gproxy-codex-{kind}:{provider_id}:{user_id}"));
    let suffix = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("gproxy-{kind}-{suffix}")
}

fn random_url(length: usize) -> Result<String, OAuthError> {
    let mut bytes = vec![0_u8; length];
    getrandom::fill(&mut bytes).map_err(|_| OAuthError::TemporarilyUnavailable)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn user_code() -> Result<String, OAuthError> {
    let mut bytes = [0_u8; 5];
    getrandom::fill(&mut bytes).map_err(|_| OAuthError::TemporarilyUnavailable)?;
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut code = String::with_capacity(9);
    for (index, byte) in bytes.into_iter().enumerate() {
        if index == 2 {
            code.push('-');
        }
        code.push(char::from(ALPHABET[usize::from(byte) % ALPHABET.len()]));
        code.push(char::from(
            ALPHABET[(usize::from(byte) / ALPHABET.len()) % ALPHABET.len()],
        ));
    }
    Ok(code)
}

fn normalize_code(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn field<'a>(value: &'a serde_json::Value, name: &str) -> Result<&'a str, OAuthError> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .ok_or(OAuthError::InvalidGrant)
}

fn cookie<'a>(headers: &'a http::HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(http::header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&format!("{name}=")))
        .filter(|value| !value.is_empty())
}

fn now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system time is after epoch")
        .as_secs() as i64
}

fn store(error: gproxy_store::StoreError) -> OAuthError {
    OAuthError::Store(error.to_string())
}

fn store_error(error: gproxy_store::StoreError) -> OAuthError {
    OAuthError::Store(error.to_string())
}
