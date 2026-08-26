use sha2::{Digest, Sha256};

use crate::continuation::ContinuationKey;
use crate::error::CoreError;
use crate::host::{CacheBackend, Host};

use super::super::FunnelCtx;

const LEASE_TTL: std::time::Duration = std::time::Duration::from_secs(300);
pub(super) const RENEW_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

pub(super) struct Lease {
    key: String,
    token: Vec<u8>,
}

pub(super) async fn claim<H: Host>(
    host: &H,
    channel: &dyn gproxy_channel_api::Channel,
    ctx: &FunnelCtx,
    id: &str,
) -> Result<Lease, CoreError> {
    let key = ContinuationKey {
        channel: channel.descriptor().id,
        provider_id: ctx.target.provider.id,
        owner_user_id: ctx.owner_user_id.unwrap_or_default(),
        id: id.into(),
    };
    let digest = Sha256::digest(key.id.as_bytes());
    let mut hash = String::with_capacity(digest.len() * 2);
    use std::fmt::Write as _;
    for byte in digest {
        write!(hash, "{byte:02x}").expect("writing to String");
    }
    let owner = format!(
        "gproxy:session-owner:{}:{}:{hash}",
        key.channel, key.provider_id
    );
    let token = Sha256::digest(format!(
        "{}:{}:{}:{}:{}",
        key.channel, key.provider_id, key.owner_user_id, key.id, ctx.request_id
    ))
    .to_vec();
    if !host
        .cache()
        .compare_and_swap(&owner, None, Some(token.clone()), Some(LEASE_TTL))
        .await?
    {
        return Err(CoreError::Internal(
            "Realtime sideband already has an owner".into(),
        ));
    }
    Ok(Lease { key: owner, token })
}

pub(super) async fn renew<H: Host>(host: &H, lease: &Lease) -> Result<(), CoreError> {
    let renewed = host
        .cache()
        .compare_and_swap(
            &lease.key,
            Some(lease.token.clone()),
            Some(lease.token.clone()),
            Some(LEASE_TTL),
        )
        .await?;
    if renewed {
        Ok(())
    } else {
        Err(CoreError::Internal(
            "Realtime sideband ownership lease was lost".into(),
        ))
    }
}

pub(super) async fn release<H: Host>(host: &H, lease: &Lease) {
    let result = host
        .cache()
        .compare_and_swap(&lease.key, Some(lease.token.clone()), None, None)
        .await;
    if let Err(error) = result {
        tracing::error!(error = %error, "release Realtime sideband owner failed");
    }
}
