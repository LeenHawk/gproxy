use crate::{Core, CoreError, CredentialId, CredentialStore, Host, ProviderRef, UpstreamTransport};
use bytes::BytesMut;
use futures_util::StreamExt;
use gproxy_channel_api::{
    QuotaEntry, QuotaQueryMode, QuotaResetCredits, QuotaSource, QuotaSupport,
};

pub struct QuotaSourceProbeResult {
    pub entries: Vec<QuotaEntry>,
    pub reset_credits: Option<QuotaResetCredits>,
    pub raw: String,
    pub observed_at_ms: i64,
}

impl<H: Host> Core<H> {
    pub fn quota_sources(
        &self,
        channel: &str,
        secret: &serde_json::Value,
        settings: &serde_json::Value,
    ) -> Result<Vec<QuotaSource>, CoreError> {
        let Some(channel) = self.channels.get(channel) else {
            return Ok(vec![QuotaSource {
                id: "quota".into(),
                label: "Upstream quota".into(),
                kinds: Vec::new(),
                mode: QuotaQueryMode::Unavailable,
                support: QuotaSupport::Unsupported,
                reason: Some("This provider's channel is no longer registered.".into()),
                automatic: false,
            }]);
        };
        Ok(channel.quota_sources(&scoped_secret(channel.descriptor().id, secret), settings))
    }

    pub async fn quota_source(
        &self,
        provider: &ProviderRef,
        credential: CredentialId,
        expected_version: u64,
        source_id: &str,
    ) -> Result<QuotaSourceProbeResult, CoreError> {
        let channel = self
            .channels
            .get(&provider.channel)
            .ok_or_else(|| CoreError::UnknownProvider("channel is not registered".into()))?;
        let record = self.host.credentials().load(credential).await?;
        if record.version != expected_version || record.channel != provider.channel {
            return Err(CoreError::Unsupported);
        }
        let query_secret = scoped_secret(&provider.channel, &record.secret);
        let supported = channel
            .quota_sources(&query_secret, &provider.settings)
            .iter()
            .any(|source| {
                source.id == source_id
                    && source.support == QuotaSupport::Ready
                    && source.mode == QuotaQueryMode::Probe
            });
        if !supported {
            return Err(CoreError::Unsupported);
        }
        if source_id == "subscription" {
            let result = self
                .quota_probe_version(
                    &provider.channel,
                    provider,
                    credential,
                    Some(expected_version),
                )
                .await?;
            if result.observations.is_empty() && result.reset_credits.is_none() {
                return Err(CoreError::UpstreamExhausted(
                    "quota endpoint returned no valid quota data".into(),
                ));
            }
            let observed_at_ms = crate::quota::now_ms();
            return Ok(QuotaSourceProbeResult {
                entries: result
                    .observations
                    .iter()
                    .map(|value| QuotaEntry::from_window(value, observed_at_ms))
                    .collect(),
                reset_credits: result.reset_credits,
                raw: result.raw,
                observed_at_ms,
            });
        }
        let mut entries = Vec::new();
        let mut cursor = None;
        let mut cursors = std::collections::HashSet::new();
        let mut ids = std::collections::HashSet::new();
        for _ in 0..100 {
            let mut request = channel
                .prepare_quota_source_page(
                    source_id,
                    &query_secret,
                    &provider.settings,
                    cursor.as_deref(),
                )?
                .ok_or(CoreError::Unsupported)?;
            crate::fingerprint::apply_request(&mut request, provider)?;
            let response = self.host.transport().send(request).await?;
            let (parts, mut stream) = response.into_parts();
            if !parts.status.is_success() {
                if let Some(seconds) = parts
                    .headers
                    .get(http::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u32>().ok())
                {
                    return Err(CoreError::RateLimited {
                        retry_after_secs: seconds,
                    });
                }
                return Err(CoreError::UpstreamExhausted(format!(
                    "quota endpoint returned HTTP {}",
                    parts.status
                )));
            }
            let mut body = BytesMut::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                if body.len() + chunk.len() > 4 * 1024 * 1024 {
                    return Err(CoreError::UpstreamExhausted(
                        "quota response exceeds 4 MiB".into(),
                    ));
                }
                body.extend_from_slice(&chunk);
            }
            let page = channel
                .parse_quota_source_page(source_id, parts.status, &parts.headers, &body)
                .map_err(|_| {
                    CoreError::UpstreamExhausted("quota endpoint returned invalid data".into())
                })?;
            let observed_at_ms = crate::quota::now_ms();
            for mut entry in page.entries {
                if !ids.insert(entry.id.clone()) || entries.len() >= 10000 {
                    return Err(CoreError::UpstreamExhausted(
                        "quota pages contain duplicate entries or exceed the result limit".into(),
                    ));
                }
                entry.source_id = source_id.to_owned();
                entry.observed_at_ms = observed_at_ms;
                entries.push(entry);
            }
            match page.next_cursor {
                Some(next) if !next.is_empty() && cursors.insert(next.clone()) => {
                    cursor = Some(next)
                }
                Some(_) => {
                    return Err(CoreError::UpstreamExhausted(
                        "quota pagination returned a repeated or empty cursor".into(),
                    ));
                }
                None => {
                    return Ok(QuotaSourceProbeResult {
                        entries,
                        reset_credits: None,
                        raw: String::new(),
                        observed_at_ms,
                    });
                }
            }
        }
        Err(CoreError::UpstreamExhausted(
            "quota query exceeds 100 pages".into(),
        ))
    }
}

fn scoped_secret<'a>(
    channel: &str,
    secret: &'a serde_json::Value,
) -> std::borrow::Cow<'a, serde_json::Value> {
    if secret
        .get("quota_channel")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|bound| bound != channel)
    {
        let mut value = secret.clone();
        if let Some(object) = value.as_object_mut() {
            object.retain(|key, _| !key.starts_with("quota_"));
        }
        std::borrow::Cow::Owned(value)
    } else {
        std::borrow::Cow::Borrowed(secret)
    }
}
