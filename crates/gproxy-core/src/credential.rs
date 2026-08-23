use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use gproxy_channel_api::{BoxFuture, Channel, ChannelError, SimpleHttp};

use crate::error::CoreError;
use crate::host::{CredentialId, CredentialRecord, CredentialStore, Host, UpstreamTransport};

const REFRESH_LEASE_TTL: Duration = Duration::from_secs(60);

pub(crate) async fn load_fresh<H: Host>(
    host: &H,
    channel: &dyn Channel,
    id: CredentialId,
) -> Result<CredentialRecord, CoreError> {
    let channel_id = channel.descriptor().id;
    let record = load_checked(host, id, channel_id).await?;
    let now = unix_now()?;
    if !refresh_due(channel, &record, now) {
        return Ok(record);
    }

    if !host
        .credentials()
        .lease_refresh(id, REFRESH_LEASE_TTL)
        .await?
    {
        return load_checked(host, id, channel_id).await;
    }

    let current = load_checked(host, id, channel_id).await?;
    if !refresh_due(channel, &current, unix_now()?) {
        return Ok(current);
    }
    let http = BufferedHttp(host.transport());
    let replacement = channel
        .refresh(&current.secret, &http)
        .ok_or_else(|| ChannelError::Refresh("channel did not provide a refresh operation".into()))?
        .await?;

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

async fn load_checked<H: Host>(
    host: &H,
    id: CredentialId,
    channel: &str,
) -> Result<CredentialRecord, CoreError> {
    let record = host.credentials().load(id).await?;
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

struct BufferedHttp<'a, T: ?Sized>(&'a T);

impl<T: UpstreamTransport + ?Sized> SimpleHttp for BufferedHttp<'_, T> {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, ChannelError>> {
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
