use std::sync::Arc;

use crate::app::AppState;
use crate::channel::{
    Channel, ChannelError, Disposition, RateLimitResetCreditConsumeResponse, UsageSnapshot,
};
use crate::http::client::UpstreamClient;
use crate::store::persistence::records::{Credential, Provider};

use super::UsageError;

#[derive(Debug)]
pub(super) struct UsageFailure {
    pub(super) error: UsageError,
    pub(super) disposition: Option<Disposition>,
}

impl UsageFailure {
    fn classified(error: UsageError, disposition: Disposition) -> Self {
        Self {
            error,
            disposition: Some(disposition),
        }
    }

    fn unclassified(error: UsageError) -> Self {
        Self {
            error,
            disposition: None,
        }
    }
}

pub(super) async fn fetch_with(
    channel: &Arc<dyn Channel>,
    secret: &serde_json::Value,
    settings: &serde_json::Value,
    client: &Arc<dyn UpstreamClient>,
) -> Result<UsageSnapshot, UsageFailure> {
    let Some(req) = channel
        .prepare_usage_request(secret, settings)
        .map_err(classify_channel_error)?
    else {
        return Err(UsageFailure::unclassified(UsageError::Unsupported));
    };
    let resp = client.send(req).await.map_err(|e| {
        UsageFailure::classified(UsageError::Upstream(e.to_string()), Disposition::Transient)
    })?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.into_body();
    let disposition = channel.classify(status, &headers, &body);
    if disposition != Disposition::Success {
        return Err(UsageFailure::classified(
            UsageError::Status(status.as_u16()),
            disposition,
        ));
    }
    channel.parse_usage(status, &headers, &body).ok_or_else(|| {
        UsageFailure::classified(UsageError::Status(status.as_u16()), Disposition::Transient)
    })
}

pub(super) async fn consume_reset_credit_with(
    channel: &Arc<dyn Channel>,
    secret: &serde_json::Value,
    settings: &serde_json::Value,
    client: &Arc<dyn UpstreamClient>,
    idempotency_key: &str,
) -> Result<RateLimitResetCreditConsumeResponse, UsageFailure> {
    let Some(req) = channel
        .prepare_rate_limit_reset_credit_request(secret, settings, idempotency_key)
        .map_err(classify_channel_error)?
    else {
        return Err(UsageFailure::unclassified(UsageError::Unsupported));
    };
    let resp = client.send(req).await.map_err(|e| {
        UsageFailure::classified(UsageError::Upstream(e.to_string()), Disposition::Transient)
    })?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.into_body();
    let disposition = channel.classify(status, &headers, &body);
    if disposition != Disposition::Success {
        return Err(UsageFailure::classified(
            UsageError::Status(status.as_u16()),
            disposition,
        ));
    }
    channel
        .parse_rate_limit_reset_credit(status, &headers, &body)
        .ok_or_else(|| {
            UsageFailure::classified(UsageError::Status(status.as_u16()), Disposition::Transient)
        })
}

fn classify_channel_error(error: ChannelError) -> UsageFailure {
    let disposition =
        matches!(error, ChannelError::InvalidCredential(_)).then_some(Disposition::AuthDead);
    UsageFailure {
        error: UsageError::Channel(error),
        disposition,
    }
}

pub(super) fn finish<T>(
    state: &AppState,
    provider: &Provider,
    credential: &Credential,
    result: Result<T, UsageFailure>,
) -> Result<T, UsageError> {
    match result {
        Ok(value) => {
            super::record(state, provider, credential, &Disposition::Success);
            Ok(value)
        }
        Err(failure) => {
            if let Some(disposition) = &failure.disposition {
                super::record(state, provider, credential, disposition);
            }
            Err(failure.error)
        }
    }
}
