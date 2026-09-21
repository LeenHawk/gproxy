use crate::{
    transform::TransformError,
    wire::{gemini::video as g, openai::video as o},
};

/// OpenRouter provider options are keyed by the documented provider slug.
/// Only selected Google AI Studio options are used for Developer API Veo.
pub(super) fn to_gemini(
    provider: Option<&o::VideoProvider>,
    out: &mut g::VideoGenerationParameters,
) -> Result<(), TransformError> {
    let Some(options) = provider
        .and_then(|v| v.options.as_ref())
        .and_then(|v| v.get("google-ai-studio"))
    else {
        return Ok(());
    };
    for (key, value) in options {
        match key.as_str() {
            "negative_prompt" | "negativePrompt" => assign(&mut out.negative_prompt, value, key)?,
            "person_generation" | "personGeneration" => {
                assign(&mut out.person_generation, value, key)?
            }
            "enhance_prompt" | "enhancePrompt" => assign(&mut out.enhance_prompt, value, key)?,
            "duration_seconds" | "durationSeconds" => {
                assign(&mut out.duration_seconds, value, key)?
            }
            "aspect_ratio" | "aspectRatio" => assign(&mut out.aspect_ratio, value, key)?,
            "resolution" => assign(&mut out.resolution, value, key)?,
            "sample_count" | "sampleCount" => {
                let count: i32 = decode(value, key)?;
                if count <= 0 {
                    return Err(TransformError::shape(
                        "sample_count",
                        "positive sample count required",
                    ));
                }
                out.sample_count = Some(count);
            }
            _ => {
                return Err(TransformError::unsupported(
                    format!("provider.options.google-ai-studio.{key}"),
                    "option has no verified Developer API Veo mapping",
                ));
            }
        }
    }
    Ok(())
}

fn decode<T: serde::de::DeserializeOwned>(
    value: &serde_json::Value,
    key: &str,
) -> Result<T, TransformError> {
    serde_json::from_value(value.clone())
        .map_err(|e| TransformError::shape(format!("provider.options.{key}"), e.to_string()))
}

fn assign<T: serde::de::DeserializeOwned + PartialEq>(
    target: &mut Option<T>,
    value: &serde_json::Value,
    key: &str,
) -> Result<(), TransformError> {
    let value = decode(value, key)?;
    if target.as_ref().is_some_and(|v| v != &value) {
        return Err(TransformError::shape(
            key,
            "standard field conflicts with provider option",
        ));
    }
    *target = Some(value);
    Ok(())
}

pub(super) fn to_openrouter(input: &g::VideoGenerationParameters) -> Option<o::VideoProvider> {
    let mut options = serde_json::Map::new();
    if let Some(count) = input.sample_count {
        options.insert("sample_count".into(), count.into());
    }
    if let Some(v) = &input.negative_prompt {
        options.insert("negative_prompt".into(), v.clone().into());
    }
    if let Some(v) = &input.person_generation {
        options.insert("person_generation".into(), v.clone().into());
    }
    if let Some(v) = input.enhance_prompt {
        options.insert("enhance_prompt".into(), v.into());
    }
    if options.is_empty() {
        None
    } else {
        Some(
            o::VideoProvider::builder()
                .options(std::collections::BTreeMap::from([(
                    "google-ai-studio".into(),
                    options,
                )]))
                .build(),
        )
    }
}
