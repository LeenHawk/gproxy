use std::time::Duration;

use gproxy_channel_api::{CallerIdentity, SurfaceAffinity};

use crate::api::Core;
use crate::boundary::{ExecOutcome, ResponseBody};
use crate::control::Target;
use crate::error::CoreError;
use crate::host::{CacheBackend, CredentialId, Host};

pub(crate) struct AffinityPin {
    key: String,
    value: Vec<u8>,
    ttl: Duration,
}

pub(crate) struct TokenBinding {
    pub credential: CredentialId,
    pub identity: CallerIdentity,
}

pub(crate) async fn cached<H: Host>(
    core: &Core<H>,
    first: &Target,
    candidates: &[&Target],
    key: String,
    ttl_secs: u64,
) -> Result<Option<(Target, Option<AffinityPin>)>, CoreError> {
    if let Some(credential) = core.host.cache().get(&key).await?.and_then(decode_id)
        && let Some(target) = candidates
            .iter()
            .find(|target| target.credential == credential)
    {
        return Ok(Some((
            (*target).clone(),
            Some(AffinityPin {
                key,
                value: credential.0.to_be_bytes().to_vec(),
                ttl: Duration::from_secs(ttl_secs),
            }),
        )));
    }
    Ok(Some((
        (*first).clone(),
        Some(AffinityPin {
            key,
            value: first.credential.0.to_be_bytes().to_vec(),
            ttl: Duration::from_secs(ttl_secs),
        }),
    )))
}

pub(crate) async fn commit<H: Host>(core: &Core<H>, pin: AffinityPin) -> Result<(), CoreError> {
    core.host
        .cache()
        .set(&pin.key, pin.value, Some(pin.ttl))
        .await?;
    Ok(())
}

pub(crate) fn response_pins(
    affinity: SurfaceAffinity,
    identity: &CallerIdentity,
    target: &Target,
    outcome: &ExecOutcome,
) -> Vec<AffinityPin> {
    if let SurfaceAffinity::ResponseBodyToken {
        field,
        namespace,
        also_body_field,
        also_path_field,
        ttl_secs,
        ..
    } = affinity
    {
        let ResponseBody::Full(body) = &outcome.body else {
            return Vec::new();
        };
        let Ok(body) = serde_json::from_slice::<serde_json::Value>(body) else {
            return Vec::new();
        };
        let mut pins = Vec::new();
        if let Some(value) = body.get(field).cloned().and_then(value_key) {
            pins.push(AffinityPin {
                key: token_key(target.provider.id, namespace, &value),
                value: encode_token(target.credential, identity),
                ttl: Duration::from_secs(ttl_secs),
            });
        }
        if let Some(name) = also_body_field
            && let Some(value) = body.get(name).cloned().and_then(value_key)
        {
            pins.push(AffinityPin {
                key: cache_key(target, identity, "body", name, &value),
                value: target.credential.0.to_be_bytes().to_vec(),
                ttl: Duration::from_secs(ttl_secs),
            });
        }
        if let Some(name) = also_path_field
            && let Some(value) = body.get(name).cloned().and_then(value_key)
        {
            pins.push(AffinityPin {
                key: cache_key(target, identity, "path", name, &value),
                value: target.credential.0.to_be_bytes().to_vec(),
                ttl: Duration::from_secs(ttl_secs),
            });
        }
        return pins;
    }
    let pin = match affinity {
        SurfaceAffinity::Header { name, ttl_secs } => (
            "header",
            name,
            outcome
                .headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned),
            ttl_secs,
        ),
        SurfaceAffinity::BodyField { name, ttl_secs } => {
            let ResponseBody::Full(body) = &outcome.body else {
                return Vec::new();
            };
            let value = serde_json::from_slice::<serde_json::Value>(body)
                .ok()
                .and_then(|body| body.get(name).cloned())
                .and_then(value_key);
            ("body", name, value, ttl_secs)
        }
        SurfaceAffinity::HeaderOrBodyField {
            body_field,
            ttl_secs,
            ..
        } => {
            let ResponseBody::Full(body) = &outcome.body else {
                return Vec::new();
            };
            let value = serde_json::from_slice::<serde_json::Value>(body)
                .ok()
                .and_then(|body| body.get(body_field).cloned())
                .and_then(value_key);
            ("body", body_field, value, ttl_secs)
        }
        SurfaceAffinity::None
        | SurfaceAffinity::PathParam { .. }
        | SurfaceAffinity::ResponseBodyToken { .. }
        | SurfaceAffinity::BearerToken { .. }
        | SurfaceAffinity::Binding { .. } => return Vec::new(),
    };
    let (source, name, Some(value), ttl_secs) = pin else {
        return Vec::new();
    };
    vec![AffinityPin {
        key: cache_key(target, identity, source, name, &value),
        value: target.credential.0.to_be_bytes().to_vec(),
        ttl: Duration::from_secs(ttl_secs),
    }]
}

pub(crate) fn token_key(provider_id: i64, namespace: &str, value: &str) -> String {
    format!(
        "gproxy:surface-token:{provider_id}:{}",
        pin_hash("token", namespace, value)
    )
}

pub(crate) fn decode_token(value: Vec<u8>) -> Option<TokenBinding> {
    let mut chunks = value.chunks_exact(8);
    let values = {
        let mut next = || Some(i64::from_be_bytes(chunks.next()?.try_into().ok()?));
        [next()?, next()?, next()?, next()?, next()?]
    };
    if !chunks.remainder().is_empty() || chunks.next().is_some() {
        return None;
    }
    let [credential, user_id, user_key_id, org_id, team_id] = values;
    Some(TokenBinding {
        credential: CredentialId(credential),
        identity: CallerIdentity {
            user_id,
            user_key_id,
            org_id: (org_id != i64::MIN).then_some(org_id),
            team_id: (team_id != i64::MIN).then_some(team_id),
        },
    })
}

fn encode_token(credential: CredentialId, identity: &CallerIdentity) -> Vec<u8> {
    [
        credential.0,
        identity.user_id,
        identity.user_key_id,
        identity.org_id.unwrap_or(i64::MIN),
        identity.team_id.unwrap_or(i64::MIN),
    ]
    .into_iter()
    .flat_map(i64::to_be_bytes)
    .collect()
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
