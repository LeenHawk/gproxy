use std::time::Duration;

use web_time::{SystemTime, UNIX_EPOCH};

use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use gproxy_channel_api::{BoxFuture, Channel, ChannelError, SimpleHttp};

use crate::control::ProviderRef;
use crate::error::CoreError;
use crate::host::{CredentialId, CredentialRecord, CredentialStore, Host, UpstreamTransport};

const REFRESH_LEASE_TTL: Duration = Duration::from_secs(60);
const REFRESH_POLL_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) async fn load_fresh<H: Host>(
    host: &H,
    channel: &dyn Channel,
    id: CredentialId,
    provider: &ProviderRef,
) -> Result<CredentialRecord, CoreError> {
    load(host, channel, id, provider, false).await
}

pub(crate) async fn refresh_now<H: Host>(
    host: &H,
    channel: &dyn Channel,
    id: CredentialId,
    provider: &ProviderRef,
) -> Result<CredentialRecord, CoreError> {
    load(host, channel, id, provider, true).await
}

async fn load<H: Host>(
    host: &H,
    channel: &dyn Channel,
    id: CredentialId,
    provider: &ProviderRef,
    force: bool,
) -> Result<CredentialRecord, CoreError> {
    let channel_id = channel.descriptor().id;
    let record = load_checked(host, id, channel_id).await?;
    let now = unix_now()?;
    if !force && !refresh_due(channel, &record, now) {
        return Ok(record);
    }

    let mut observed_version = record.version;
    while !host
        .credentials()
        .lease_refresh(id, REFRESH_LEASE_TTL)
        .await?
    {
        if let Some(peer) = wait_for_peer(host, channel, id, channel_id, observed_version).await? {
            return Ok(peer);
        }
        observed_version = load_checked(host, id, channel_id).await?.version;
    }

    let current = load_checked(host, id, channel_id).await?;
    if !force && !refresh_due(channel, &current, unix_now()?) {
        return Ok(current);
    }
    let http = BufferedHttp(host.transport(), provider);
    let refresh = channel
        .refresh(&current.secret, &provider.settings, &http)
        .ok_or_else(|| {
            ChannelError::Refresh("channel did not provide a refresh operation".into())
        })?;
    let replacement = match refresh.await {
        Ok(replacement) => replacement,
        Err(error) => {
            host.record_credential_health(
                id,
                current.version,
                crate::CredentialHealth::Degraded,
                None,
                "credential refresh failed",
            )
            .await;
            return Err(error.into());
        }
    };

    match host
        .credentials()
        .persist_rotation(id, replacement, current.version)
        .await
    {
        Ok(()) => load_checked(host, id, channel_id).await,
        Err(error) => {
            let peer = load_checked(host, id, channel_id).await?;
            if peer.version != current.version && !refresh_due(channel, &peer, unix_now()?) {
                Ok(peer)
            } else {
                Err(error.into())
            }
        }
    }
}

async fn wait_for_peer<H: Host>(
    host: &H,
    channel: &dyn Channel,
    id: CredentialId,
    channel_id: &str,
    observed_version: u64,
) -> Result<Option<CredentialRecord>, CoreError> {
    let polls = REFRESH_LEASE_TTL.as_secs() / REFRESH_POLL_INTERVAL.as_secs();
    for _ in 0..polls {
        host.wait(REFRESH_POLL_INTERVAL).await;
        let peer = load_checked(host, id, channel_id).await?;
        if peer.version != observed_version && !refresh_due(channel, &peer, unix_now()?) {
            return Ok(Some(peer));
        }
    }
    Ok(None)
}

async fn load_checked<H: Host>(
    host: &H,
    id: CredentialId,
    channel: &str,
) -> Result<CredentialRecord, CoreError> {
    let record = host.credentials().load(id).await?;
    if record.id != id {
        return Err(CoreError::Internal(
            "credential store returned the wrong credential".into(),
        ));
    }
    if record.channel != channel {
        return Err(CoreError::Internal(
            "credential channel does not match its provider".into(),
        ));
    }
    Ok(record)
}

fn refresh_due(channel: &dyn Channel, record: &CredentialRecord, now: i64) -> bool {
    channel
        .refresh_due(&record.secret)
        .is_some_and(|due| due <= now)
}

fn unix_now() -> Result<i64, CoreError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CoreError::Internal("system clock is before the Unix epoch".into()))?
        .as_secs();
    i64::try_from(seconds).map_err(|_| CoreError::Internal("Unix time exceeds i64".into()))
}

struct BufferedHttp<'a, T: ?Sized>(&'a T, &'a ProviderRef);

impl<T: UpstreamTransport + ?Sized> SimpleHttp for BufferedHttp<'_, T> {
    fn send<'a>(
        &'a self,
        mut request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, ChannelError>> {
        if let Err(error) = crate::fingerprint::apply_request(&mut request, self.1) {
            return Box::pin(async move { Err(ChannelError::Refresh(error.to_string())) });
        }
        let send = self.0.send(request);
        Box::pin(async move {
            let response = send
                .await
                .map_err(|error| ChannelError::Refresh(error.to_string()))?;
            let (parts, mut stream) = response.into_parts();
            let mut body = BytesMut::new();
            while let Some(chunk) = stream.next().await {
                body.extend_from_slice(
                    &chunk.map_err(|error| ChannelError::Refresh(error.to_string()))?,
                );
            }
            Ok(http::Response::from_parts(parts, body.freeze()))
        })
    }
}
