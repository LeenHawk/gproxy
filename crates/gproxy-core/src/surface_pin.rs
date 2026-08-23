use std::time::Duration;

use gproxy_channel_api::{CallerIdentity, SurfaceAffinity};

use crate::api::Core;
use crate::boundary::{ExecOutcome, ResponseBody};
use crate::control::Target;
use crate::error::CoreError;
use crate::host::{CacheBackend, CredentialId, Host};

pub(crate) struct AffinityPin {
    key: String,
    credential: CredentialId,
    ttl: Duration,
}

pub(crate) async fn cached<H: Host>(
    core: &Core<H>,
    first: &Target,
    candidates: &[&Target],
    key: String,
    ttl_secs: u64,
) -> Result<Option<(Target, Option<AffinityPin>)>, CoreError> {
    if let Some(credential) = core.host.cache().get(&key).await.and_then(decode_id)
        && let Some(target) = candidates
            .iter()
            .find(|target| target.credential == credential)
    {
        return Ok(Some((
            (*target).clone(),
            Some(AffinityPin {
                key,
                credential,
                ttl: Duration::from_secs(ttl_secs),
            }),
        )));
    }
    Ok(Some((
        (*first).clone(),
        Some(AffinityPin {
            key,
            credential: first.credential,
            ttl: Duration::from_secs(ttl_secs),
        }),
    )))
}

pub(crate) async fn commit<H: Host>(core: &Core<H>, pin: AffinityPin) {
    core.host
        .cache()
        .set(
            &pin.key,
            pin.credential.0.to_be_bytes().to_vec(),
            Some(pin.ttl),
        )
        .await;
}

pub(crate) fn response_pin(
    affinity: SurfaceAffinity,
    identity: &CallerIdentity,
    target: &Target,
    outcome: &ExecOutcome,
) -> Option<AffinityPin> {
    let (source, name, value, ttl_secs) = match affinity {
        SurfaceAffinity::Header { name, ttl_secs } => (
            "header",
            name,
            outcome.headers.get(name)?.to_str().ok()?.to_owned(),
            ttl_secs,
        ),
        SurfaceAffinity::BodyField { name, ttl_secs } => {
            let ResponseBody::Full(body) = &outcome.body else {
                return None;
            };
            let body = serde_json::from_slice::<serde_json::Value>(body).ok()?;
            ("body", name, value_key(body.get(name)?.clone())?, ttl_secs)
        }
        SurfaceAffinity::None | SurfaceAffinity::Binding { .. } => return None,
    };
    Some(AffinityPin {
        key: cache_key(target, identity, source, name, &value),
        credential: target.credential,
        ttl: Duration::from_secs(ttl_secs),
    })
}

pub(crate) fn cache_key(
    target: &Target,
    identity: &CallerIdentity,
    source: &str,
    name: &str,
    value: &str,
) -> String {
    format!(
        "gproxy:surface:{}:{}:{}",
        target.provider.id,
        identity.user_key_id,
        pin_hash(source, name, value)
    )
}

fn pin_hash(source: &str, name: &str, value: &str) -> String {
    use std::fmt::Write;

    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    for part in [source, name, value] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    let mut encoded = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut encoded, "{byte:02x}").expect("writing to String succeeds");
    }
    encoded
}

fn decode_id(value: Vec<u8>) -> Option<CredentialId> {
    Some(CredentialId(i64::from_be_bytes(value.try_into().ok()?)))
}

pub(crate) fn value_key(value: serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}
