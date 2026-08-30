use gproxy_channel_api::{AuthCodeStart, BoxFuture, DeviceInit, DevicePoll, MaybeSend, MaybeSync};
use gproxy_store::records::CredentialEnvelope;

use crate::dto::{
    ChannelDto, ConnectivityTestRequest, ConnectivityTestResponse, ExportSourceKeyDto,
    ModelTestRequest, ModelTestResponse, PortalModelDto, TokenizerVocabDto,
};
use crate::{AdminError, PortalIdentity};

pub trait State: MaybeSend + MaybeSync {
    fn store(&self) -> &gproxy_store::Store;

    fn seal_credential(&self, secret: &serde_json::Value)
    -> Result<CredentialEnvelope, AdminError>;

    fn seal_user_key(&self, api_key: &str) -> Result<CredentialEnvelope, AdminError>;

    fn reseal_imported_credential(
        &self,
        envelope: &CredentialEnvelope,
        source: &ExportSourceKeyDto,
        source_master_key: Option<&str>,
    ) -> Result<CredentialEnvelope, AdminError>;

    fn reseal_imported_user_key(
        &self,
        envelope: &CredentialEnvelope,
        source: &ExportSourceKeyDto,
        source_master_key: Option<&str>,
    ) -> Result<CredentialEnvelope, AdminError>;

    fn digest_user_key(&self, api_key: &str) -> (u32, Vec<u8>);

    fn reveal_user_key(
        &self,
        actor_user_id: i64,
        id: i64,
        at: i64,
    ) -> BoxFuture<'_, Result<String, AdminError>>;

    fn admit_auth_attempt(
        &self,
        scope: &'static str,
        username: &str,
    ) -> BoxFuture<'_, Result<(), AdminError>>;

    fn clear_auth_attempts(
        &self,
        scope: &'static str,
        username: &str,
    ) -> BoxFuture<'_, Result<(), AdminError>>;

    fn reload(&self) -> BoxFuture<'_, Result<(), AdminError>>;

    fn connectivity_test<'a>(
        &'a self,
        request: &'a ConnectivityTestRequest,
    ) -> BoxFuture<'a, Result<ConnectivityTestResponse, AdminError>>;

    /// Real inference, so it goes through the same funnel as any request: the
    /// operator's own key pays for it and it appears in usage like everything else.
    fn test_model<'a>(
        &'a self,
        actor_user_id: i64,
        request: &'a ModelTestRequest,
    ) -> BoxFuture<'a, Result<ModelTestResponse, AdminError>>;

    fn fetch_tokenizer_vocab<'a>(
        &'a self,
        name: &'a str,
    ) -> BoxFuture<'a, Result<TokenizerVocabDto, AdminError>> {
        let _ = name;
        Box::pin(async { Err(AdminError::Forbidden) })
    }

    fn delete_tokenizer_vocab<'a>(
        &'a self,
        name: &'a str,
    ) -> BoxFuture<'a, Result<(), AdminError>> {
        let _ = name;
        Box::pin(async { Err(AdminError::Forbidden) })
    }

    fn login_state_get<'a>(
        &'a self,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, AdminError>>;

    fn login_state_set<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl: std::time::Duration,
    ) -> BoxFuture<'a, Result<(), AdminError>>;

    fn login_state_delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), AdminError>>;

    fn login_authcode_start<'a>(
        &'a self,
        channel: &'a str,
        provider_id: i64,
        params: &'a serde_json::Value,
        redirect_uri: &'a str,
        flow_state: &'a str,
        pkce_challenge: &'a str,
    ) -> BoxFuture<'a, Result<Option<AuthCodeStart>, AdminError>>;

    fn login_authcode_exchange<'a>(
        &'a self,
        channel: &'a str,
        provider_id: i64,
        code: &'a str,
        verifier: &'a str,
        redirect_uri: &'a str,
        extra: Option<&'a serde_json::Value>,
    ) -> BoxFuture<'a, Result<serde_json::Value, AdminError>>;

    fn login_device_start<'a>(
        &'a self,
        channel: &'a str,
        provider_id: i64,
        params: &'a serde_json::Value,
    ) -> BoxFuture<'a, Result<DeviceInit, AdminError>>;

    fn login_device_poll<'a>(
        &'a self,
        channel: &'a str,
        provider_id: i64,
        device_code: &'a str,
    ) -> BoxFuture<'a, Result<DevicePoll, AdminError>>;

    fn login_cookie_exchange<'a>(
        &'a self,
        channel: &'a str,
        provider_id: i64,
        cookie: &'a str,
    ) -> BoxFuture<'a, Result<serde_json::Value, AdminError>>;

    fn channel_catalogue(&self) -> Vec<ChannelDto>;

    fn portal_identity(&self, headers: &http::HeaderMap) -> Result<PortalIdentity, AdminError>;

    fn portal_models(&self, identity: &PortalIdentity) -> Vec<PortalModelDto>;

    fn normalize_provider_settings(
        &self,
        channel: &str,
        settings: &serde_json::Value,
    ) -> Result<serde_json::Value, AdminError>;
}
