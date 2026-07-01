use crate::protocol::{claude, openai};

use super::DEFAULT_MODEL;

pub(super) fn openai_previous_response_id_to_claude(
    previous_response_id: Option<String>,
) -> Option<claude::DiagnosticsParam> {
    Some(claude::DiagnosticsParam {
        previous_message_id: Some(Some(previous_response_id?)),
        extra: Default::default(),
    })
}

pub(super) fn compact_service_tier_to_claude(
    service_tier: Option<openai::CompactServiceTier>,
) -> Option<claude::RequestServiceTier> {
    let service_tier = match service_tier? {
        openai::CompactServiceTier::Auto => claude::RequestServiceTierKnown::Auto,
        openai::CompactServiceTier::Default => claude::RequestServiceTierKnown::StandardOnly,
        openai::CompactServiceTier::Flex | openai::CompactServiceTier::Priority => {
            claude::RequestServiceTierKnown::Auto
        }
    };
    Some(claude::RequestServiceTier::Known(service_tier))
}

pub(super) fn model_to_string<T: serde::Serialize>(model: &T) -> String {
    let Ok(value) = serde_json::to_value(model) else {
        return DEFAULT_MODEL.to_owned();
    };
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| DEFAULT_MODEL.to_owned())
}

pub(super) fn join_text(parts: impl Iterator<Item = String>) -> String {
    parts
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
