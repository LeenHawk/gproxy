use std::time::Duration;

use gproxy_channel_api::{Channel, claude_fallback_setting};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::api::Core;
use crate::control::Target;
use crate::host::{CacheBackend, Host};

pub(crate) async fn lookup<H: Host>(
    core: &Core<H>,
    channel: &dyn Channel,
    target: &Target,
    session: Option<&str>,
    body: &[u8],
) -> (Option<String>, Option<String>) {
    let Some(session) = session.filter(|_| channel.claude_fallback().is_some()) else {
        return (None, None);
    };
    let wire = serde_json::from_slice::<Value>(body).ok();
    let setting = wire
        .as_ref()
        .and_then(|body| body.get("fallbacks"))
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| claude_fallback_setting(&target.provider.settings));
    let Some(setting) = setting else {
        return (None, None);
    };
    if wire
        .as_ref()
        .is_some_and(gproxy_channel_api::has_fallback_credit)
    {
        return (None, None);
    }
    let scope = serde_json::json!([
        target.provider.id,
        target.credential.0,
        session,
        target.upstream_model,
        setting
    ]);
    let digest = bytes::Bytes::copy_from_slice(&Sha256::digest(scope.to_string().as_bytes()));
    let key = format!("gproxy:refusal-model:{digest:x}");
    let model = match core.host.cache().get(&key).await {
        Ok(Some(bytes)) => String::from_utf8(bytes).ok(),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(error = %error, "fallback model affinity lookup failed");
            None
        }
    };
    (Some(key), model)
}

pub(super) async fn save<H: Host>(core: &Core<H>, key: Option<&str>, model: &str) {
    if let Some(key) = key
        && let Err(error) = core
            .host
            .cache()
            .set(
                key,
                model.as_bytes().to_vec(),
                Some(Duration::from_secs(3600)),
            )
            .await
    {
        tracing::warn!(error = %error, "fallback model affinity persistence failed");
    }
}
