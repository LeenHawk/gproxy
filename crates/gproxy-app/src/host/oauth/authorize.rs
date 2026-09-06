use gproxy_channel_api::{
    CODEX_OAUTH_CLIENT_ID, GPROXY_OAUTH_SCOPE, OAuthAuthorizeInput, OAuthBrowserUser,
    OAuthCodeGrant, OAuthError, OAuthService,
};
use gproxy_store::records::{OAuthAuthorizationInput, UserKeyInput};

use super::super::AppHost;
use super::util::{digest, now, random_url, stable_id, store};

pub(super) async fn create(
    host: &AppHost,
    user: &OAuthBrowserUser,
    input: OAuthAuthorizeInput,
) -> Result<OAuthCodeGrant, OAuthError> {
    let client = host.client(&input.client_id).await?;
    if input.client_id != CODEX_OAUTH_CLIENT_ID && input.scopes != [GPROXY_OAUTH_SCOPE] {
        return Err(OAuthError::InvalidRequest);
    }
    let api_key = random_url(48)?;
    let digest_version = crate::control::USER_KEY_DIGEST_VERSION;
    let key_digest = crate::control::user_key_digest(digest_version, &api_key)
        .ok_or(OAuthError::TemporarilyUnavailable)?;
    let envelope = host
        .services
        .cipher
        .seal_user_key(&serde_json::Value::String(api_key))
        .map_err(|error| OAuthError::Store(error.to_string()))?;
    let key = UserKeyInput {
        user_id: user.identity.user_id,
        digest: key_digest,
        digest_version,
        prefix: "oauth".into(),
        envelope,
        label: Some(format!("OAuth: {}", client.name)),
        expires_at: None,
        enabled: true,
    };
    let current = now();
    let provider = input.provider_id.unwrap_or(0);
    let code = random_url(32)?;
    if !host
        .services
        .store
        .create_oauth_authorization(&OAuthAuthorizationInput {
            key,
            provider_id: input.provider_id,
            client_id: input.client_id,
            scopes: input.scopes.join(" "),
            chatgpt_user_id: if client.client_id == CODEX_OAUTH_CLIENT_ID {
                stable_id("user", provider, user.identity.user_id)
            } else {
                String::new()
            },
            chatgpt_account_id: if client.client_id == CODEX_OAUTH_CLIENT_ID {
                stable_id("account", provider, user.identity.user_id)
            } else {
                String::new()
            },
            created_at: current,
            code_digest: digest(&code),
            redirect_uri: input.redirect_uri,
            code_challenge: input.code_challenge,
            expires_at: current + super::CODE_SECONDS,
        })
        .await
        .map_err(store)?
    {
        return Err(OAuthError::InvalidClient);
    }
    host.services.control.reload().await.map_err(store)?;
    Ok(OAuthCodeGrant { code })
}
