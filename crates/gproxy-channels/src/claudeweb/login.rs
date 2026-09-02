use gproxy_channel_api::{
    BoxFuture, ChannelError, ChannelLogin, CookieExchangeCtx, CredentialAcquisition, SimpleHttp,
};
use serde_json::json;

use super::ClaudeWebChannel;

impl ChannelLogin for ClaudeWebChannel {
    fn cookie_exchange<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: CookieExchangeCtx<'a>,
    ) -> BoxFuture<'a, Result<CredentialAcquisition, ChannelError>> {
        Box::pin(async move {
            let cookie = crate::shared::claude::cookie::normalize(ctx.cookie)
                .ok_or_else(|| ChannelError::Login("cookie is missing sessionKey".into()))?;
            let request = super::auth::login_request(&cookie, ctx.provider_settings)
                .map_err(|error| ChannelError::Login(error.to_string()))?;
            let response = http
                .send(request)
                .await
                .map_err(|error| ChannelError::Login(error.to_string()))?;
            if !response.status().is_success() {
                return Err(ChannelError::Login(format!(
                    "Claude bootstrap endpoint {}",
                    response.status()
                )));
            }
            super::bootstrap::merge(&json!({ "cookie": cookie }), response.body())
                .map(CredentialAcquisition::cookie)
                .map_err(|error| ChannelError::Login(error.to_string()))
        })
    }
}
