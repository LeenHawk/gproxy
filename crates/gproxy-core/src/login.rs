use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use gproxy_channel_api::{
    AuthCodeExchangeCtx, AuthCodeStart, AuthCodeStartCtx, ChannelError, CookieExchangeCtx,
    DeviceInit, DevicePoll, DevicePollCtx, DeviceStartCtx, SimpleHttp,
};
use serde_json::Value;

use crate::{BoxFuture, Core, Host, ProviderRef, UpstreamTransport};

impl<H: Host> Core<H> {
    pub async fn login_authcode_start(
        &self,
        channel: &str,
        provider: &ProviderRef,
        params: &Value,
        redirect_uri: &str,
        state: &str,
        pkce_challenge: &str,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        let login = self.login(channel, provider)?;
        login
            .adapter
            .authcode_start(
                &LoginHttp(self.host.transport(), provider),
                AuthCodeStartCtx {
                    provider_settings: &provider.settings,
                    params,
                    redirect_uri,
                    state,
                    pkce_challenge,
                },
            )
            .await
    }

    pub async fn login_authcode_exchange(
        &self,
        channel: &str,
        provider: &ProviderRef,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        extra: Option<&Value>,
    ) -> Result<Value, ChannelError> {
        self.login(channel, provider)?
            .adapter
            .authcode_exchange(
                &LoginHttp(self.host.transport(), provider),
                AuthCodeExchangeCtx {
                    provider_settings: &provider.settings,
                    code,
                    verifier,
                    redirect_uri,
                    extra,
                },
            )
            .await
    }

    pub async fn login_device_start(
        &self,
        channel: &str,
        provider: &ProviderRef,
        params: &Value,
    ) -> Result<DeviceInit, ChannelError> {
        self.login(channel, provider)?
            .adapter
            .device_start(
                &LoginHttp(self.host.transport(), provider),
                DeviceStartCtx {
                    provider_settings: &provider.settings,
                    params,
                },
            )
            .await
    }

    pub async fn login_device_poll(
        &self,
        channel: &str,
        provider: &ProviderRef,
        device_code: &str,
    ) -> Result<DevicePoll, ChannelError> {
        self.login(channel, provider)?
            .adapter
            .device_poll(
                &LoginHttp(self.host.transport(), provider),
                DevicePollCtx {
                    provider_settings: &provider.settings,
                    device_code,
                },
            )
            .await
    }

    pub async fn login_cookie_exchange(
        &self,
        channel: &str,
        provider: &ProviderRef,
        cookie: &str,
    ) -> Result<Value, ChannelError> {
        self.login(channel, provider)?
            .adapter
            .cookie_exchange(
                &LoginHttp(self.host.transport(), provider),
                CookieExchangeCtx {
                    provider_settings: &provider.settings,
                    cookie,
                },
            )
            .await
    }

    fn login<'a>(
        &'a self,
        channel: &str,
        provider: &ProviderRef,
    ) -> Result<gproxy_channel_api::ChannelLoginRef<'a>, ChannelError> {
        if provider.channel != channel {
            return Err(ChannelError::Login(
                "login provider does not match channel".into(),
            ));
        }
        self.channels
            .login_for(channel)
            .ok_or(ChannelError::Unsupported("channel login"))
    }
}

struct LoginHttp<'a, T: ?Sized>(&'a T, &'a ProviderRef);

impl<T: UpstreamTransport + ?Sized> SimpleHttp for LoginHttp<'_, T> {
    fn send<'a>(
        &'a self,
        mut request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, ChannelError>> {
        if let Err(error) = crate::fingerprint::apply_request(&mut request, self.1) {
            return Box::pin(async move { Err(ChannelError::Login(error.to_string())) });
        }
        let send = self.0.send(request);
        Box::pin(async move {
            let response = send
                .await
                .map_err(|error| ChannelError::Login(error.to_string()))?;
            let (parts, mut stream) = response.into_parts();
            let mut body = BytesMut::new();
            while let Some(chunk) = stream.next().await {
                body.extend_from_slice(
                    &chunk.map_err(|error| ChannelError::Login(error.to_string()))?,
                );
            }
            Ok(http::Response::from_parts(parts, body.freeze()))
        })
    }
}
