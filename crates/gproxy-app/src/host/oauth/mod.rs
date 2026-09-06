mod authorize;
pub(crate) mod device;
mod tokens;
mod util;

use gproxy_channel_api::{
    BoxFuture, CallerIdentity, OAuthAuthorizeInput, OAuthBrowserUser, OAuthClientInfo,
    OAuthCodeGrant, OAuthDevicePoll, OAuthDeviceStart, OAuthError, OAuthService, OAuthTokenSet,
};

use super::AppHost;
use util::cookie;
pub(crate) use util::{digest, now, store};

pub(crate) const ACCESS_SECONDS: i64 = 3600;
pub(crate) const REFRESH_SECONDS: i64 = 30 * 24 * 3600;
pub(crate) const CODE_SECONDS: i64 = 300;
pub(crate) const DEVICE_SECONDS: i64 = 900;

impl OAuthService for AppHost {
    fn client<'a>(
        &'a self,
        client_id: &'a str,
    ) -> BoxFuture<'a, Result<OAuthClientInfo, OAuthError>> {
        Box::pin(async move {
            let record = self
                .services
                .store
                .oauth_client(client_id)
                .await
                .map_err(store)?
                .filter(|record| record.enabled && record.deleted_at.is_none())
                .ok_or(OAuthError::InvalidClient)?;
            Ok(OAuthClientInfo {
                client_id: record.client_id,
                name: record.name,
                redirect_uris: record.redirect_uris,
            })
        })
    }

    fn browser_user<'a>(
        &'a self,
        headers: &'a http::HeaderMap,
    ) -> BoxFuture<'a, Result<Option<OAuthBrowserUser>, OAuthError>> {
        Box::pin(async move {
            let Some(token) = cookie(headers, "gproxy_portal_session") else {
                return Ok(None);
            };
            let user = self
                .services
                .store
                .user_for_session(&digest(token), now())
                .await
                .map_err(store)?;
            Ok(user.map(|user| OAuthBrowserUser {
                identity: CallerIdentity {
                    oauth_access_digest: None,
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
        Box::pin(async move {
            let client = self.client(&input.client_id).await?;
            if !gproxy_channel_api::oauth_redirect_allowed(
                &client.redirect_uris,
                &input.redirect_uri,
            ) {
                return Err(OAuthError::InvalidRequest);
            }
            authorize::create(self, user, input).await
        })
    }

    fn exchange_code<'a>(
        &'a self,
        code: &'a str,
        client_id: &'a str,
        redirect_uri: &'a str,
        verifier: &'a str,
        issuer: &'a str,
    ) -> BoxFuture<'a, Result<OAuthTokenSet, OAuthError>> {
        Box::pin(tokens::exchange_code(
            self,
            code,
            client_id,
            redirect_uri,
            verifier,
            issuer,
        ))
    }

    fn refresh<'a>(
        &'a self,
        refresh_token: &'a str,
        client_id: &'a str,
        issuer: &'a str,
    ) -> BoxFuture<'a, Result<OAuthTokenSet, OAuthError>> {
        Box::pin(tokens::refresh(self, refresh_token, client_id, issuer))
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
        provider_id: Option<i64>,
        client_id: &'a str,
        _issuer: &'a str,
    ) -> BoxFuture<'a, Result<OAuthDeviceStart, OAuthError>> {
        Box::pin(device::start(self, provider_id, client_id))
    }

    fn device_poll<'a>(
        &'a self,
        device_auth_id: &'a str,
        user_code: &'a str,
    ) -> BoxFuture<'a, Result<OAuthDevicePoll, OAuthError>> {
        Box::pin(device::poll(self, device_auth_id, user_code))
    }

    fn device_approve<'a>(
        &'a self,
        user: &'a OAuthBrowserUser,
        user_code: &'a str,
        issuer: &'a str,
    ) -> BoxFuture<'a, Result<(), OAuthError>> {
        Box::pin(device::approve(self, user, user_code, issuer))
    }
}
