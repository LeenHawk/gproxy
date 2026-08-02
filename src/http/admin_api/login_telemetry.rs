//! Safe structured diagnostics for admin login flows.

use crate::channel::ChannelError;

pub(super) fn channel_error_kind(error: &ChannelError) -> &'static str {
    match error {
        ChannelError::MissingSetting(_) => "missing_setting",
        ChannelError::InvalidCredential(_) => "invalid_credential",
        ChannelError::Unsupported(_) => "unsupported",
        ChannelError::Build(_) => "request_or_upstream",
        ChannelError::Transient(_) => "transient",
    }
}

pub(super) fn warn_flow_failure(
    channel: &str,
    provider_id: i64,
    operation: &'static str,
    status: u16,
    error_kind: &'static str,
) {
    tracing::warn!(
        channel,
        provider_id,
        operation,
        status,
        error_kind,
        "login flow operation failed"
    );
}
