//! OAuth for a protected resource that names its own authorization server:
//! the resource is all an operator configures, and everything else is
//! discovered (RFC 9728, RFC 8414 / OpenID Connect Discovery), registered
//! (RFC 7591) and bound to the resource (RFC 8707).
//!
//! [`ProtectedResourceOAuth`] is the sign-in and the refresh of any upstream
//! kind whose provider `base_url` is such a resource.

mod discovery;
mod http;
mod token;

pub use discovery::{AuthorizationServer, Challenge, Discovery, discover};

use std::collections::BTreeMap;

use gproxy_protocol::capability::CapabilityFuture;
use serde_json::Value;

use crate::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, CredentialRefresh,
    CredentialUpdate, CredentialView, LoginContext, OAuthAuthorizationCode, OAuthCredential,
    RefreshContext, RefreshReason, UpstreamError, UpstreamFuture,
};
use token::RegisteredClient;

/// Authorization code with PKCE against whatever authorization server the
/// provider's `base_url` names, and refresh of the tokens it issued.
#[derive(Clone, Copy, Debug)]
pub struct ProtectedResourceOAuth {
    /// The `client_name` this gateway registers itself under.
    pub client_name: &'static str,
    /// Where the browser is sent back when the host names no redirect of its
    /// own. Nothing has to listen there: the person hands the address the
    /// browser ended on back to the host.
    pub redirect_uri: &'static str,
}

fn resource_url(base_url: Option<&str>) -> Result<&str, UpstreamError> {
    base_url.ok_or_else(|| {
        UpstreamError::InvalidConfig("the provider has no base_url to sign in to".to_owned())
    })
}

impl OAuthAuthorizationCode for ProtectedResourceOAuth {
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> UpstreamFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            let resource = resource_url(context.provider.base_url)?;
            let redirect_uri = match request.redirect_uri {
                "" => self.redirect_uri,
                named => named,
            };
            let challenge = discovery::probe(context.client, resource).await?;
            let found = discover(context.client, resource, challenge.as_deref()).await?;
            let client = token::register(
                context.client,
                &found.authorization_server,
                self.client_name,
                redirect_uri,
            )
            .await?;
            let mut url = url::Url::parse(&found.authorization_server.authorization_endpoint)
                .map_err(|error| UpstreamError::InvalidResponse(error.to_string()))?;
            {
                let mut query = url.query_pairs_mut();
                query
                    .append_pair("response_type", "code")
                    .append_pair("client_id", &client.client_id)
                    .append_pair("redirect_uri", redirect_uri)
                    .append_pair("state", request.state)
                    .append_pair("code_challenge", request.code_challenge)
                    .append_pair("code_challenge_method", "S256")
                    .append_pair("resource", &found.resource);
                if let Some(scope) = &found.scope {
                    query.append_pair("scope", scope);
                }
            }
            Ok(AuthorizationStart {
                authorize_url: url.into(),
                redirect_uri: redirect_uri.to_owned(),
                provider_state: client.into_map(found.resource),
            })
        })
    }

    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> UpstreamFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let (client, resource) = RegisteredClient::from_map(grant.provider_state)?;
            let reply = token::request(
                context.client,
                &client,
                &[
                    ("grant_type", "authorization_code"),
                    ("code", grant.code),
                    ("redirect_uri", grant.redirect_uri),
                    ("code_verifier", grant.code_verifier),
                    ("resource", &resource),
                ],
            )
            .await?
            .map_err(|refusal| UpstreamError::UpstreamResponse {
                status: refusal.status,
                body: refusal.body,
            })?;
            Ok(OAuthCredential {
                access_token: reply.access_token,
                refresh_token: reply.refresh_token,
                id_token: None,
                token_type: reply.token_type,
                scopes: token::scopes(reply.scope.as_deref()),
                expires_at_ms: token::expiry(reply.expires_in),
                refresh_expires_at_ms: None,
                provider_fields: BTreeMap::new(),
                provider_secrets: grant.provider_state.clone(),
            })
        })
    }
}

impl CredentialRefresh for ProtectedResourceOAuth {
    /// Five minutes before the access token lapses, when there is a refresh
    /// token to renew it with.
    fn proactive_refresh_reason(
        &self,
        credential: CredentialView<'_>,
        now_ms: i64,
    ) -> Option<RefreshReason> {
        (self.supports(&credential)
            && credential
                .expires_at_ms
                .is_some_and(|expires| expires <= now_ms.saturating_add(300_000)))
        .then_some(RefreshReason::ProactiveExpiry)
    }

    /// A pasted secret has nothing to renew.
    fn supports(&self, credential: &CredentialView<'_>) -> bool {
        credential
            .secret
            .get("refresh_token")
            .is_some_and(Value::is_string)
    }

    fn refresh<'a>(
        &'a self,
        context: RefreshContext<'a>,
    ) -> CapabilityFuture<'a, Result<CredentialUpdate, UpstreamError>> {
        Box::pin(async move {
            let mut credential: OAuthCredential =
                serde_json::from_value(context.credential.secret.clone())
                    .map_err(|_| UpstreamError::InvalidCredential)?;
            let refresh_token = credential
                .refresh_token
                .clone()
                .ok_or(UpstreamError::InvalidCredential)?;
            let (client, resource) = RegisteredClient::from_map(&credential.provider_secrets)?;
            let reply = token::request(
                context.client,
                &client,
                &[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", &refresh_token),
                    ("resource", &resource),
                ],
            )
            .await?
            .map_err(|refusal| match refusal.definitive() {
                Some(reason) => UpstreamError::RefreshRejected(reason),
                None => UpstreamError::UpstreamResponse {
                    status: refusal.status,
                    body: refusal.body,
                },
            })?;
            credential.access_token = reply.access_token;
            credential.expires_at_ms = token::expiry(reply.expires_in);
            // A server that does not rotate leaves the old refresh token valid.
            if reply.refresh_token.is_some() {
                credential.refresh_token = reply.refresh_token;
            }
            if reply.token_type.is_some() {
                credential.token_type = reply.token_type;
            }
            if reply.scope.is_some() {
                credential.scopes = token::scopes(reply.scope.as_deref());
            }
            Ok(CredentialUpdate {
                expires_at_ms: credential.expires_at_ms,
                secret: serde_json::to_value(&credential)
                    .map_err(|error| UpstreamError::InvalidResponse(error.to_string()))?,
                metadata: None,
            })
        })
    }
}
