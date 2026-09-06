use base64::Engine as _;
use bytes::Bytes;
use gproxy_admin::dto::{
    OAuthAuthorizationRequest, OAuthAuthorizeDecision, OAuthConsentDto, OAuthRedirectDto,
};
use gproxy_channel_api::{
    CODEX_OAUTH_CLIENT_ID, GPROXY_OAUTH_SCOPE, OAuthAuthorizeInput, OAuthClientInfo, OAuthError,
    OAuthService,
};
use http::{Response, StatusCode, request::Parts};

use super::wire;
use crate::host::AppHost;

pub(super) async fn start(host: &AppHost, parts: &Parts) -> Result<Response<Bytes>, OAuthError> {
    let request: OAuthAuthorizationRequest = wire::query(parts)?;
    validate(host, &request).await?;
    wire::redirect(&format!(
        "/portal?oauth_authorize={}",
        wire::encode(parts.uri.query().unwrap_or_default())
    ))
}

pub(super) async fn details(host: &AppHost, parts: &Parts) -> Result<Response<Bytes>, OAuthError> {
    let user = wire::browser(host, parts, false).await?;
    let request: OAuthAuthorizationRequest = wire::query(parts)?;
    let client = validate(host, &request).await?;
    Ok(wire::json_response(
        StatusCode::OK,
        &OAuthConsentDto {
            client_id: client.client_id,
            client_name: client.name,
            user_name: user.name,
            scope: request.scope,
            user_code: None,
        },
    ))
}

pub(super) async fn decide(
    host: &AppHost,
    parts: &Parts,
    body: &[u8],
) -> Result<Response<Bytes>, OAuthError> {
    let user = wire::browser(host, parts, true).await?;
    let decision: OAuthAuthorizeDecision = wire::parse(parts, body)?;
    let request = decision.authorization;
    validate(host, &request).await?;
    let result = if decision.approved {
        let grant = host
            .authorize(
                &user,
                OAuthAuthorizeInput {
                    provider_id: None,
                    client_id: request.client_id,
                    redirect_uri: request.redirect_uri.clone(),
                    scopes: request
                        .scope
                        .split_ascii_whitespace()
                        .map(str::to_owned)
                        .collect(),
                    code_challenge: request.code_challenge,
                },
            )
            .await?;
        format!("code={}", wire::encode(&grant.code))
    } else {
        "error=access_denied".into()
    };
    let separator = if request.redirect_uri.contains('?') {
        '&'
    } else {
        '?'
    };
    Ok(wire::json_response(
        StatusCode::OK,
        &OAuthRedirectDto {
            redirect_uri: format!(
                "{}{separator}{result}&state={}",
                request.redirect_uri,
                wire::encode(&request.state)
            ),
        },
    ))
}

async fn validate(
    host: &AppHost,
    request: &OAuthAuthorizationRequest,
) -> Result<OAuthClientInfo, OAuthError> {
    let client = host.client(&request.client_id).await?;
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&request.code_challenge)
        .map_err(|_| OAuthError::InvalidRequest)?;
    if request.response_type != "code"
        || request.code_challenge_method != "S256"
        || challenge.len() != 32
        || request.state.is_empty()
        || request.state.len() > 1024
        || !gproxy_channel_api::oauth_redirect_allowed(&client.redirect_uris, &request.redirect_uri)
    {
        return Err(OAuthError::InvalidRequest);
    }
    let scopes = request.scope.split_ascii_whitespace().collect::<Vec<_>>();
    let valid_scopes = if request.client_id == CODEX_OAUTH_CLIENT_ID {
        !scopes.is_empty()
            && scopes.iter().all(|scope| {
                [
                    "openid",
                    "profile",
                    "email",
                    "offline_access",
                    "api.connectors.read",
                    "api.connectors.invoke",
                ]
                .contains(scope)
            })
    } else {
        scopes == [GPROXY_OAUTH_SCOPE]
    };
    if !valid_scopes {
        return Err(OAuthError::InvalidRequest);
    }
    Ok(client)
}
