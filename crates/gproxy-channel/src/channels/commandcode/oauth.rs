//! Browser authorization using the official manual-transfer page.
//! The first-party website tries a loopback JSON POST when mode=redirect is
//! omitted, then displays the issued key on /studio/auth/cli/fallback if that
//! transfer fails. cli-oG08m84n.js and fallback-DP3aZbqU.js under the audited
//! sample's web/ directory are the wire sources.
//!
//! Reuse the host's existing authorization-result completion. No socket is
//! opened by GProxy: the browser and gateway can run on different machines.
//! Command Code returns a long-lived key, so completion validates it using
//! /alpha/whoami rather than inventing an OAuth token endpoint or refresh token.

use super::{CommandCode, config::base_url, request};
use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, ChannelError, LoginContext,
    OAuthAuthorizationCode, OAuthCredential, OperationFuture,
};
use crate::channels::shared::compatible::ability::{require_success, send};
use http::Method;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::Value;
use std::collections::BTreeMap;

const AUTHORIZE_URL: &str = "https://commandcode.ai/studio/auth/cli";
const CALLBACK: &str = "http://127.0.0.1:5959/callback";

impl OAuthAuthorizationCode for CommandCode {
    fn authorize<'a>(
        &'a self,
        _: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            if !request.redirect_uri.trim().is_empty() {
                return Err(ChannelError::InvalidConfig(
                    "Command Code uses manual authorization-result completion; omit redirect_uri"
                        .into(),
                ));
            }
            Ok(AuthorizationStart {
                authorize_url: format!(
                    "{AUTHORIZE_URL}?callback={}&state={}",
                    utf8_percent_encode(CALLBACK, NON_ALPHANUMERIC),
                    utf8_percent_encode(request.state, NON_ALPHANUMERIC)
                ),
                // Empty selects the host's existing bare-result input. The
                // Console submits the original state alongside the result.
                redirect_uri: String::new(),
                provider_state: BTreeMap::new(),
            })
        })
    }

    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let key = grant.code.trim();
            if key.is_empty() {
                return Err(ChannelError::InvalidCredential);
            }
            // Same validation as CLI validateCommandApiKey; do not trust
            // account fields copied from a callback or return the key as metadata.
            let (status, _, body) = send(
                context.client,
                Method::GET,
                &format!("{}/alpha/whoami", base_url(context.provider)),
                request::headers(key)?,
                None,
            )
            .await?;
            let account: Value =
                serde_json::from_slice(&require_success(status, body)?).map_err(|_| {
                    ChannelError::InvalidResponse("invalid Command Code account response".into())
                })?;
            let mut provider_fields = BTreeMap::new();
            for (name, path) in [
                ("account_id", "/user/id"),
                ("email", "/user/email"),
                ("user_name", "/user/userName"),
                ("organization_id", "/org/id"),
                ("organization_name", "/org/name"),
            ] {
                if let Some(value) = account
                    .pointer(path)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    provider_fields.insert(name.into(), Value::String(value.into()));
                }
            }
            Ok(OAuthCredential {
                access_token: key.into(),
                refresh_token: None,
                id_token: None,
                token_type: Some("Bearer".into()),
                scopes: Vec::new(),
                expires_at_ms: None,
                refresh_expires_at_ms: None,
                provider_fields,
                provider_secrets: BTreeMap::new(),
            })
        })
    }
}
