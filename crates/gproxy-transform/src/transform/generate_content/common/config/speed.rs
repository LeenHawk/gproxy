use crate::protocol::{claude, gemini, openai};

pub(in crate::transform::generate_content) fn openai_service_tier_to_claude_speed(
    service_tier: Option<openai::ServiceTier>,
) -> Option<claude::Speed> {
    match service_tier {
        Some(openai::ServiceTier::Fast | openai::ServiceTier::Priority) => {
            Some(claude::Speed::Known(claude::SpeedKnown::Fast))
        }
        _ => None,
    }
}

pub(in crate::transform::generate_content) fn claude_speed_to_openai(
    speed: Option<claude::Speed>,
) -> Option<openai::ServiceTier> {
    match speed {
        Some(claude::Speed::Known(claude::SpeedKnown::Fast)) => Some(openai::ServiceTier::Priority),
        _ => None,
    }
}

pub(in crate::transform::generate_content) fn claude_speed_to_gemini(
    speed: Option<claude::Speed>,
) -> Option<gemini::ServiceTier> {
    match speed {
        Some(claude::Speed::Known(claude::SpeedKnown::Fast)) => Some(gemini::ServiceTier::Known(
            gemini::ServiceTierKnown::Priority,
        )),
        _ => None,
    }
}

pub(in crate::transform::generate_content) fn gemini_service_tier_to_claude_speed(
    service_tier: Option<gemini::ServiceTier>,
) -> Option<claude::Speed> {
    match service_tier {
        Some(gemini::ServiceTier::Known(gemini::ServiceTierKnown::Priority)) => {
            Some(claude::Speed::Known(claude::SpeedKnown::Fast))
        }
        _ => None,
    }
}

pub(in crate::transform::generate_content) fn claude_usage_to_openai_service_tier(
    usage: &claude::Usage,
) -> Option<openai::ServiceTier> {
    claude_speed_to_openai(usage.speed.clone())
        .or_else(|| super::claude_usage_service_tier_to_openai(usage.service_tier.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_fast_semantics_across_protocols() {
        for tier in [openai::ServiceTier::Fast, openai::ServiceTier::Priority] {
            assert_eq!(
                openai_service_tier_to_claude_speed(Some(tier)),
                Some(claude::Speed::Known(claude::SpeedKnown::Fast))
            );
        }
        assert_eq!(
            claude_speed_to_openai(Some(claude::Speed::Known(claude::SpeedKnown::Fast))),
            Some(openai::ServiceTier::Priority)
        );
        assert_eq!(
            claude_speed_to_gemini(Some(claude::Speed::Known(claude::SpeedKnown::Fast))),
            Some(gemini::ServiceTier::Known(
                gemini::ServiceTierKnown::Priority
            ))
        );
        assert_eq!(
            gemini_service_tier_to_claude_speed(Some(gemini::ServiceTier::Known(
                gemini::ServiceTierKnown::Priority,
            ))),
            Some(claude::Speed::Known(claude::SpeedKnown::Fast))
        );
    }
}
