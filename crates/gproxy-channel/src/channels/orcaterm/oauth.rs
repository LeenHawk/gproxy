//! The desktop chooses a UUID session_id before opening /oauth/authorize and
//! exchanges it through OAuthExchangeToken after the provider callback. We
//! already know that ID, so the host can poll it without a local URI handler.
//! Anonymous probes on 2026-10-24 confirmed TOKEN_EXPIRED for an uncompleted
//! session and browser redirects for WeChat/Tencent Cloud (China) and Google
//! (international); see samples/orcaterm-1.6.6/oauth-*.json. TOKEN_EXPIRED here
//! means no completed handoff yet, not a rejected saved credential.

use super::{
    OrcaTerm, auth,
    config::{OrcaTermConfig, new_id, now_ms},
};
use crate::channel::{
    ChannelError, DeviceAuthorization, DevicePoll, LoginContext, OAuthDeviceCode, OperationFuture,
};
use serde_json::json;
use std::collections::BTreeMap;

// This is our bounded browser-login window, not an upstream token lifetime.
const LOGIN_WINDOW_MS: i64 = 10 * 60 * 1000;

impl OAuthDeviceCode for OrcaTerm {
    fn start<'a>(&'a self, ctx: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = OrcaTermConfig::from_view(ctx.provider)?;
            let session_id = new_id()?;
            let site = config.site_url();
            let mut url = url::Url::parse(&format!("{site}/oauth/authorize"))
                .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
            // Return to the existing website, avoiding an installed desktop
            // application's URI handler consuming the handoff before we do.
            url.query_pairs_mut().extend_pairs([
                ("provider", config.oauth_provider()?),
                ("source", "desktop"),
                ("session_id", session_id.as_str()),
                ("return_url", format!("{site}/").as_str()),
            ]);
            Ok(DeviceAuthorization {
                device_code: session_id,
                user_code: "No code required".into(),
                verification_uri: url.into(),
                verification_uri_complete: None,
                expires_at_ms: Some(now_ms().saturating_add(LOGIN_WINDOW_MS)),
                interval_secs: 3,
                provider_state: BTreeMap::new(),
            })
        })
    }

    fn poll<'a>(
        &'a self,
        ctx: LoginContext<'a>,
        authorization: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll> {
        Box::pin(async move {
            if authorization.expires_at_ms.is_some_and(|at| at <= now_ms()) {
                return Ok(DevicePoll::Expired);
            }
            if authorization.device_code.trim().is_empty() {
                return Err(ChannelError::InvalidCredential);
            }
            let config = OrcaTermConfig::from_view(ctx.provider)?;
            let response = auth::rpc_response(
                ctx.client,
                &config,
                None,
                "OAuthExchangeToken",
                json!({"sessionId":authorization.device_code}),
            )
            .await?;
            if response
                .pointer("/Error/Code")
                .and_then(serde_json::Value::as_str)
                == Some("TOKEN_EXPIRED")
            {
                return Ok(DevicePoll::Pending);
            }
            auth::check_response("OAuthExchangeToken", &response)?;
            let mut credential = auth::tokens(&response)?;
            // The exchange may consume the handoff. Persist the issued token
            // even when profile enrichment is temporarily unavailable; the
            // generation path retries identity discovery before preparing chat.
            if let Ok(fields) = auth::identity(
                ctx.client,
                &config,
                &json!({"access_token":credential.access_token}),
            )
            .await
            {
                credential.provider_fields = fields;
            }
            Ok(DevicePoll::Ready(credential))
        })
    }
}
