use gproxy_protocol::{claude, gemini};

use crate::TransformError;

pub(super) fn output(
    config: Option<&gemini::GenerationConfig>,
) -> Result<Option<claude::OutputConfig>, TransformError> {
    let Some(config) = config else {
        return Ok(None);
    };
    let schema = match &config.response_json_schema {
        Some(schema) => Some(schema.clone()),
        None => config
            .response_schema
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?,
    };
    let format = schema.map(json_format).transpose()?;
    let effort = config
        .thinking_config
        .as_ref()
        .and_then(|thinking| thinking.thinking_level.as_ref())
        .and_then(effort);
    Ok(
        (format.is_some() || effort.is_some()).then_some(claude::OutputConfig {
            effort,
            format,
            task_budget: None,
            rest: Default::default(),
        }),
    )
}

pub(super) fn thinking(
    config: Option<&gemini::GenerationConfig>,
) -> Result<Option<claude::ThinkingConfig>, TransformError> {
    let Some(thinking) = config.and_then(|config| config.thinking_config.as_ref()) else {
        return Ok(None);
    };
    if thinking.include_thoughts == Some(false) {
        return Ok(Some(claude::ThinkingConfig::Disabled(
            claude::ThinkingDisabled {
                type_: claude::ThinkingDisabledType::Disabled,
                rest: thinking.rest.clone(),
            },
        )));
    }
    if let Some(budget) = thinking.thinking_budget {
        let budget_tokens = u64::try_from(budget).map_err(|_| {
            TransformError::shape("Gemini thinking config", "thinkingBudget is negative")
        })?;
        return Ok(Some(claude::ThinkingConfig::Enabled(
            claude::ThinkingEnabled {
                budget_tokens,
                type_: claude::ThinkingEnabledType::Enabled,
                display: None,
                rest: thinking.rest.clone(),
            },
        )));
    }
    Ok(Some(claude::ThinkingConfig::Adaptive(
        claude::ThinkingAdaptive {
            type_: claude::ThinkingAdaptiveType::Adaptive,
            display: None,
            rest: thinking.rest.clone(),
        },
    )))
}

pub(super) fn request_tier(
    tier: Option<gemini::ServiceTier>,
) -> (Option<claude::RequestServiceTier>, Option<claude::Speed>) {
    let speed = matches!(
        tier,
        Some(gemini::ServiceTier::Known(
            gemini::ServiceTierKnown::Priority
        ))
    )
    .then_some(claude::Speed::Known(claude::SpeedKnown::Fast));
    let request = tier.map(|tier| match tier {
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Standard) => {
            claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::StandardOnly)
        }
        gemini::ServiceTier::Known(
            gemini::ServiceTierKnown::Flex
            | gemini::ServiceTierKnown::Priority
            | gemini::ServiceTierKnown::Unspecified,
        ) => claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::Auto),
        gemini::ServiceTier::Unknown(value) => claude::RequestServiceTier::Unknown(value),
        _ => claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::Auto),
    });
    (request, speed)
}

fn json_format(value: serde_json::Value) -> Result<claude::JsonSchemaFormat, TransformError> {
    let serde_json::Value::Object(schema) = value else {
        return Err(TransformError::shape(
            "Gemini response schema",
            "expected an object",
        ));
    };
    Ok(claude::JsonSchemaFormat {
        type_: claude::JsonSchemaFormatType::Known(claude::JsonSchemaFormatTypeKnown::JsonSchema),
        schema,
        rest: Default::default(),
    })
}

fn effort(level: &gemini::ThinkingLevel) -> Option<claude::OutputEffort> {
    Some(claude::OutputEffort::Known(match level {
        gemini::ThinkingLevel::Known(
            gemini::ThinkingLevelKnown::Minimal | gemini::ThinkingLevelKnown::Low,
        ) => claude::OutputEffortKnown::Low,
        gemini::ThinkingLevel::Known(
            gemini::ThinkingLevelKnown::Medium
            | gemini::ThinkingLevelKnown::ThinkingLevelUnspecified,
        ) => claude::OutputEffortKnown::Medium,
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::High) => {
            claude::OutputEffortKnown::High
        }
        gemini::ThinkingLevel::Unknown(_) => return None,
        _ => return None,
    }))
}
