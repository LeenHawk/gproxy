//! On-demand quota probe: query a channel's dedicated usage endpoint for one
//! credential and sink the windows through the same observation path as
//! header-borne readings. Callers trigger this explicitly — some upstreams
//! rate-limit their usage endpoints aggressively.

use base64::Engine as _;
use bytes::BytesMut;
use futures_util::StreamExt;
use gproxy_channel_api::{QuotaObservation, QuotaResetCredits, QuotaResetResult};

use crate::host::{CredentialId, CredentialStore};
use crate::{Core, CoreError, Host, ProviderRef, UpstreamTransport};

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaProbeResult {
    pub observations: Vec<QuotaObservation>,
    pub reset_credits: Option<QuotaResetCredits>,
    /// Verbatim usage-endpoint body, so an operator can inspect windows the
    /// channel parser does not (yet) extract.
    pub raw: String,
}

impl<H: Host> Core<H> {
    pub fn quota_capabilities(
        &self,
        channel: &str,
        secret: &serde_json::Value,
    ) -> Result<Option<gproxy_channel_api::QuotaCapabilities>, CoreError> {
        let channel = self
            .channels
            .get(channel)
            .ok_or_else(|| CoreError::UnknownProvider("channel is not registered".into()))?;
        Ok(channel.quota_capabilities(secret))
    }

    pub async fn quota_probe(
        &self,
        channel: &str,
        provider: &ProviderRef,
        credential: CredentialId,
    ) -> Result<QuotaProbeResult, CoreError> {
        if provider.channel != channel {
            return Err(CoreError::UnknownProvider(
                "probe provider does not match channel".into(),
            ));
        }
        let channel = self
            .channels
            .get(channel)
            .ok_or_else(|| CoreError::UnknownProvider("channel is not registered".into()))?;
        let record = self.host.credentials().load(credential).await?;
        if !channel
            .quota_capabilities(&record.secret)
            .is_some_and(|capability| capability.probe)
        {
            return Err(CoreError::Unsupported);
        }
        let Some(mut request) = channel.prepare_quota_probe(&record.secret, &provider.settings)?
        else {
            return Err(CoreError::Unsupported);
        };
        crate::fingerprint::apply_request(&mut request, provider)?;
        let started_at_ms = now_ms();
        let (status, body) = self.buffered(request).await?;
        let received_at_ms = now_ms();
        if !status.is_success() {
            return Err(CoreError::UpstreamExhausted(format!(
                "usage endpoint returned HTTP {status}"
            )));
        }
        let raw = String::from_utf8_lossy(&body).into_owned();
        let mut observations = channel.parse_quota_probe(status, &body);
        for observation in &mut observations {
            observation.sample = Some(gproxy_channel_api::QuotaSample {
                source: gproxy_channel_api::QuotaSampleSource::Probe,
                started_at_ms,
                received_at_ms,
            });
        }
        let mut reset_credits = channel.parse_quota_probe_credits(status, &body);
        // The dedicated credits endpoint carries per-credit expiry the usage
        // summary lacks; a failed detail call keeps the summary's count.
        if let Some(mut request) =
            channel.prepare_quota_credits_probe(&record.secret, &provider.settings)?
        {
            crate::fingerprint::apply_request(&mut request, provider)?;
            if let Ok((status, body)) = self.buffered(request).await
                && let Some(credits) = channel.parse_quota_probe_credits(status, &body)
            {
                reset_credits = Some(credits);
            }
        }
        if !observations.is_empty() {
            self.host
                .observe_credential_quota(credential, observations.clone())
                .await;
        }
        Ok(QuotaProbeResult {
            observations,
            reset_credits,
            raw,
        })
    }

    pub async fn quota_reset(
        &self,
        channel: &str,
        provider: &ProviderRef,
        credential: CredentialId,
    ) -> Result<QuotaResetResult, CoreError> {
        if provider.channel != channel {
            return Err(CoreError::UnknownProvider(
                "reset provider does not match channel".into(),
            ));
        }
        let channel = self
            .channels
            .get(channel)
            .ok_or_else(|| CoreError::UnknownProvider("channel is not registered".into()))?;
        let record = self.host.credentials().load(credential).await?;
        let redeem_request_id = redeem_request_id()?;
        if !channel
            .quota_capabilities(&record.secret)
            .is_some_and(|capability| capability.reset)
        {
            return Err(CoreError::Unsupported);
        }
        let Some(mut request) =
            channel.prepare_quota_reset(&record.secret, &provider.settings, &redeem_request_id)?
        else {
            return Err(CoreError::Unsupported);
        };
        crate::fingerprint::apply_request(&mut request, provider)?;
        let (status, body) = self.buffered(request).await?;
        channel.parse_quota_reset(status, &body).ok_or_else(|| {
            CoreError::UpstreamExhausted(format!(
                "quota reset returned an invalid {status} response"
            ))
        })
    }

    async fn buffered(
        &self,
        request: http::Request<bytes::Bytes>,
    ) -> Result<(http::StatusCode, BytesMut), CoreError> {
        let response = self.host.transport().send(request).await?;
        let (parts, mut stream) = response.into_parts();
        if !parts.status.is_success()
            && let Some(retry_after_secs) = parts
                .headers
                .get(http::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u32>().ok())
        {
            return Err(CoreError::RateLimited { retry_after_secs });
        }
        let mut body = BytesMut::new();
        while let Some(chunk) = stream.next().await {
            body.extend_from_slice(&chunk?);
        }
        Ok((parts.status, body))
    }
}

fn redeem_request_id() -> Result<String, CoreError> {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes)
        .map_err(|_| CoreError::Internal("secure randomness unavailable".into()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

pub(crate) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock is after Unix epoch")
        .as_millis() as i64
}
