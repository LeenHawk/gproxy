//! Browser PKCE flow used by the Devin CLI. Endpoint and token contracts:
//! samples/CLIProxyAPI/internal/auth/devin/devin_auth.go.

use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, ChannelError, LoginContext,
    OAuthAuthorizationCode, OAuthCredential, OperationFuture,
};
use gproxy_protocol::{HttpBody, connection::Bytes};
use serde_json::{Value, json};

use super::{Devin, DevinConfig};

impl OAuthAuthorizationCode for Devin {
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            let config = DevinConfig::from_view(context.provider)?;
            let mut url = url::Url::parse(&config.authorize_url)
                .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
            {
                let mut query = url.query_pairs_mut();
                if !request.redirect_uri.is_empty() {
                    query.append_pair("redirect_uri", request.redirect_uri);
                }
                query
                    .append_pair("state", request.state)
                    .append_pair("prompt", "select_account")
                    .append_pair("code_challenge", request.code_challenge)
                    .append_pair("code_challenge_method", "S256");
                if request.redirect_uri.is_empty() {
                    query.append_pair("cli_pkce_marker", "1");
                }
            }
            Ok(AuthorizationStart {
                authorize_url: url.into(),
                redirect_uri: request.redirect_uri.to_owned(),
                provider_state: Default::default(),
            })
        })
    }

    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let config = DevinConfig::from_view(context.provider)?;
            let body = json!({
                "code": grant.code.trim(),
                "code_verifier": grant.code_verifier,
            });
            let request = http::Request::post(&config.token_url)
                .header(http::header::CONTENT_TYPE, "application/json")
                .header(http::header::ACCEPT, "application/json")
                .body(HttpBody::Bytes(Bytes::from(body.to_string())))
                .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
            let (status, body) = super::call(context.client, request).await?;
            if !status.is_success() {
                return Err(super::error::from_response(status, &body));
            }
            let value: Value = serde_json::from_slice(&body).map_err(|_| {
                ChannelError::InvalidResponse("invalid Devin token response".into())
            })?;
            let token = value
                .get("token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    ChannelError::InvalidResponse("Devin token response has no token".into())
                })?;
            // Only raw JWTs need the CLI session prefix; opaque keys stay intact.
            let access_token = if token.starts_with("eyJ") {
                format!("devin-session-token${token}")
            } else {
                token.to_owned()
            };
            Ok(OAuthCredential {
                access_token,
                refresh_token: None,
                id_token: None,
                token_type: None,
                scopes: Vec::new(),
                expires_at_ms: None,
                refresh_expires_at_ms: None,
                provider_fields: Default::default(),
                provider_secrets: Default::default(),
            })
        })
    }
}
