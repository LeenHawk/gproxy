use gproxy_channel_api::{BoxFuture, MaybeSend, MaybeSync};
use gproxy_store::records::CredentialEnvelope;

use crate::AdminError;
use crate::dto::ChannelDto;

pub trait State: MaybeSend + MaybeSync {
    fn store(&self) -> &gproxy_store::Store;

    fn seal_credential(&self, secret: &serde_json::Value)
    -> Result<CredentialEnvelope, AdminError>;

    fn seal_user_key(&self, api_key: &str) -> Result<CredentialEnvelope, AdminError>;

    fn digest_user_key(&self, api_key: &str) -> (u32, Vec<u8>);

    fn reveal_user_key(
        &self,
        actor_admin_id: i64,
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

    fn channel_catalogue(&self) -> Vec<ChannelDto>;

    fn normalize_provider_settings(
        &self,
        channel: &str,
        settings: &serde_json::Value,
    ) -> Result<serde_json::Value, AdminError>;
}
