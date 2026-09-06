use gproxy_channel_api::{
    CODEX_OAUTH_CLIENT_ID, GPROXY_OAUTH_SCOPE, OAuthAuthorizeInput, OAuthBrowserUser,
    OAuthDevicePoll, OAuthDeviceStart, OAuthError, OAuthService,
};
use gproxy_store::records::OAuthDeviceInput;

use super::super::AppHost;
use super::util::{digest, field, now, pkce, random_url, store};

pub(super) async fn start(
    host: &AppHost,
    provider_id: Option<i64>,
    client_id: &str,
) -> Result<OAuthDeviceStart, OAuthError> {
    host.client(client_id).await?;
    let device_auth_id = random_url(32)?;
    let user_code = user_code()?;
    let current = now();
    if !host
        .services
        .store
        .start_oauth_device(&OAuthDeviceInput {
            device_digest: digest(&device_auth_id),
            user_code: user_code.clone(),
            client_id: client_id.into(),
            provider_id,
            created_at: current,
            expires_at: current + super::DEVICE_SECONDS,
        })
        .await
        .map_err(store)?
    {
        return Err(OAuthError::InvalidClient);
    }
    Ok(OAuthDeviceStart {
        device_auth_id,
        user_code,
        interval_secs: 5,
    })
}

pub(super) async fn poll(
    host: &AppHost,
    device_auth_id: &str,
    user_code: &str,
) -> Result<OAuthDevicePoll, OAuthError> {
    let record = host
        .services
        .store
        .oauth_device_by_digest(&digest(device_auth_id))
        .await
        .map_err(store)?
        .filter(|record| record.user_code == normalize_code(user_code))
        .ok_or(OAuthError::AccessDenied)?;
    if record.expires_at <= now() || record.denied_at.is_some() {
        return Ok(OAuthDevicePoll::Denied);
    }
    host.client(&record.client_id).await?;
    let Some(envelope) = record.envelope else {
        return Ok(OAuthDevicePoll::Pending);
    };
    let value = host
        .services
        .cipher
        .open_user_key(&envelope)
        .map_err(|error| OAuthError::Store(error.to_string()))?;
    Ok(OAuthDevicePoll::Ready {
        authorization_code: field(&value, "authorization_code")?.into(),
        code_verifier: field(&value, "code_verifier")?.into(),
        code_challenge: field(&value, "code_challenge")?.into(),
    })
}

pub(super) async fn approve(
    host: &AppHost,
    user: &OAuthBrowserUser,
    user_code: &str,
    issuer: &str,
) -> Result<(), OAuthError> {
    let current = now();
    let record = host
        .services
        .store
        .oauth_device_by_code(&normalize_code(user_code))
        .await
        .map_err(store)?
        .filter(|record| {
            record.expires_at > current
                && record.approved_at.is_none()
                && record.denied_at.is_none()
        })
        .ok_or(OAuthError::InvalidRequest)?;
    host.client(&record.client_id).await?;
    let verifier = random_url(48)?;
    let challenge = pkce(&verifier);
    let legacy = record.client_id == CODEX_OAUTH_CLIENT_ID;
    let scopes = if legacy {
        vec![
            "openid",
            "profile",
            "email",
            "offline_access",
            "api.connectors.read",
            "api.connectors.invoke",
        ]
    } else {
        vec![GPROXY_OAUTH_SCOPE]
    };
    let granted = super::authorize::create(
        host,
        user,
        OAuthAuthorizeInput {
            provider_id: record.provider_id,
            client_id: record.client_id,
            redirect_uri: format!("{}/deviceauth/callback", issuer.trim_end_matches('/')),
            scopes: scopes.into_iter().map(str::to_owned).collect(),
            code_challenge: challenge.clone(),
        },
    )
    .await?;
    let code = host
        .services
        .store
        .oauth_code(&digest(&granted.code))
        .await
        .map_err(store)?
        .ok_or(OAuthError::TemporarilyUnavailable)?;
    let envelope = host
        .services
        .cipher
        .seal_user_key(&serde_json::json!({
            "authorization_code":granted.code,"code_verifier":verifier,"code_challenge":challenge,
        }))
        .map_err(|error| OAuthError::Store(error.to_string()))?;
    if !host
        .services
        .store
        .approve_oauth_device(record.id, code.grant.id, &envelope, current)
        .await
        .map_err(store)?
    {
        host.services
            .store
            .revoke_oauth_grant(code.grant.id, code.grant.user_key_id, current)
            .await
            .map_err(store)?;
        return Err(OAuthError::InvalidRequest);
    }
    Ok(())
}

pub(crate) fn normalize_code(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn user_code() -> Result<String, OAuthError> {
    let mut bytes = [0_u8; 5];
    getrandom::fill(&mut bytes).map_err(|_| OAuthError::TemporarilyUnavailable)?;
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut code = String::with_capacity(11);
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
