use crate::protocol::{claude, gemini, openai};

pub(in crate::transform::count_tokens) fn claude_service_tier_to_gemini(
    service_tier: Option<claude::RequestServiceTier>,
) -> Option<gemini::ServiceTier> {
    let tier = match service_tier? {
        claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::Auto) => {
            gemini::ServiceTierKnown::Unspecified
        }
        claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::StandardOnly) => {
            gemini::ServiceTierKnown::Standard
        }
        claude::RequestServiceTier::Unknown(_) => gemini::ServiceTierKnown::Standard,
    };
    Some(gemini::ServiceTier::Known(tier))
}

pub(in crate::transform::count_tokens) fn openai_service_tier_to_gemini(
    service_tier: Option<openai::ServiceTier>,
) -> Option<gemini::ServiceTier> {
    let tier = match service_tier? {
        openai::ServiceTier::Auto => gemini::ServiceTierKnown::Unspecified,
        openai::ServiceTier::Default | openai::ServiceTier::OnDemand => {
            gemini::ServiceTierKnown::Standard
        }
        openai::ServiceTier::Flex => gemini::ServiceTierKnown::Flex,
        openai::ServiceTier::Priority => gemini::ServiceTierKnown::Priority,
        openai::ServiceTier::Scale => gemini::ServiceTierKnown::Standard,
    };
    Some(gemini::ServiceTier::Known(tier))
}

pub(in crate::transform::count_tokens) fn gemini_service_tier_to_claude(
    service_tier: Option<gemini::ServiceTier>,
) -> Option<claude::RequestServiceTier> {
    let service_tier = match service_tier? {
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Standard) => {
            claude::RequestServiceTierKnown::StandardOnly
        }
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Flex)
        | gemini::ServiceTier::Known(gemini::ServiceTierKnown::Priority)
        | gemini::ServiceTier::Known(gemini::ServiceTierKnown::Unspecified) => {
            claude::RequestServiceTierKnown::Auto
        }
        gemini::ServiceTier::Unknown(_) => claude::RequestServiceTierKnown::StandardOnly,
    };
    Some(claude::RequestServiceTier::Known(service_tier))
}

pub(in crate::transform::count_tokens) fn openai_service_tier_to_claude(
    service_tier: Option<openai::ServiceTier>,
) -> Option<claude::RequestServiceTier> {
    let service_tier = match service_tier? {
        openai::ServiceTier::Auto => claude::RequestServiceTierKnown::Auto,
        openai::ServiceTier::Default | openai::ServiceTier::OnDemand => {
            claude::RequestServiceTierKnown::StandardOnly
        }
        openai::ServiceTier::Flex | openai::ServiceTier::Scale | openai::ServiceTier::Priority => {
            claude::RequestServiceTierKnown::Auto
        }
    };
    Some(claude::RequestServiceTier::Known(service_tier))
}

pub(in crate::transform::count_tokens) fn gemini_service_tier_to_openai(
    service_tier: Option<gemini::ServiceTier>,
) -> Option<openai::ServiceTier> {
    let service_tier = match service_tier? {
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Flex) => openai::ServiceTier::Flex,
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Priority) => {
            openai::ServiceTier::Priority
        }
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Standard)
        | gemini::ServiceTier::Known(gemini::ServiceTierKnown::Unspecified)
        | gemini::ServiceTier::Unknown(_) => openai::ServiceTier::Default,
    };
    Some(service_tier)
}

pub(in crate::transform::count_tokens) fn claude_service_tier_to_openai(
    service_tier: Option<claude::RequestServiceTier>,
) -> Option<openai::ServiceTier> {
    let service_tier = match service_tier? {
        claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::Auto) => {
            openai::ServiceTier::Auto
        }
        claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::StandardOnly)
        | claude::RequestServiceTier::Unknown(_) => openai::ServiceTier::Default,
    };
    Some(service_tier)
}
