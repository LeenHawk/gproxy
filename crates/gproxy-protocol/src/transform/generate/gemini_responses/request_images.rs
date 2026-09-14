use crate::{
    transform::{Report, TransformError},
    wire::{
        gemini as g,
        openai::responses::{input as i, tools as t},
    },
};
fn unsupported(field: &'static str) -> TransformError {
    TransformError::unsupported(
        field,
        "no equivalent selected native image control; required semantics cannot be inferred from a prompt or approximate size class",
    )
}
pub(crate) fn to_responses(
    config: Option<&g::GenerationConfig>,
    out_tools: &mut Option<Vec<t::Tool>>,
    out_choice: &mut Option<i::ToolChoice>,
) -> Result<(), TransformError> {
    let Some(config) = config else {
        return Ok(());
    };
    if config.response_modalities.as_ref().is_some_and(|values| {
        values
            .iter()
            .any(|v| !matches!(v, g::Modality::Text | g::Modality::Image))
    }) {
        return Err(unsupported("response_modalities"));
    }
    if config
        .response_format
        .as_ref()
        .is_some_and(|format| format.text.is_some() || format.audio.is_some())
    {
        return Err(unsupported("response_format"));
    }
    let enabled = config
        .response_modalities
        .as_ref()
        .is_some_and(|v| v.contains(&g::Modality::Image));
    if !enabled {
        if config.image_config.is_some()
            || config
                .response_format
                .as_ref()
                .is_some_and(|v| v.image.is_some())
        {
            return Err(unsupported("image.modalities"));
        }
        return Ok(());
    }
    if let Some(image) = &config.image_config {
        if image.aspect_ratio.is_some() {
            return Err(unsupported("image_config.aspect_ratio"));
        }
        if image.image_size.is_some() {
            return Err(unsupported("image_config.image_size"));
        }
    }
    let mut tool = t::ImageGenerationTool::builder().build();
    if let Some(format) = &config.response_format {
        if format.text.is_some() || format.audio.is_some() {
            return Err(unsupported("response_format"));
        }
        if let Some(image) = &format.image {
            if image.delivery == Some(g::Delivery::Uri) {
                return Err(TransformError::unsupported(
                    "image.delivery",
                    "URI delivery requires the publication capability adapter",
                ));
            }
            if image
                .aspect_ratio
                .as_ref()
                .is_some_and(|v| *v != g::AspectRatio::Unspecified)
            {
                return Err(unsupported("image.aspect_ratio"));
            }
            if image
                .image_size
                .as_ref()
                .is_some_and(|v| *v != g::ImageSize::Unspecified)
            {
                return Err(unsupported("image.image_size"));
            }
            if image.mime_type == Some(g::ImageMimeType::ImageJpeg) {
                tool.output_format = Some(t::ImageOutputFormat::Jpeg);
            }
        }
    }
    let image_only = config
        .response_modalities
        .as_ref()
        .is_some_and(|v| !v.is_empty() && v.iter().all(|v| *v == g::Modality::Image));
    let image_selector = || {
        serde_json::json!({"type":"image_generation"})
            .as_object()
            .unwrap()
            .clone()
    };
    let choice = (*out_choice).take();
    (*out_choice) = Some(match choice {
        None | Some(i::ToolChoice::Mode(i::ToolChoiceMode::Auto)) if !image_only => {
            i::ToolChoice::Mode(i::ToolChoiceMode::Auto)
        }
        None | Some(i::ToolChoice::Mode(i::ToolChoiceMode::Auto | i::ToolChoiceMode::None))
            if image_only =>
        {
            i::ToolChoice::Hosted(
                i::ToolChoiceHosted::builder(i::ToolChoiceHostedType::ImageGeneration).build(),
            )
        }
        Some(i::ToolChoice::Mode(i::ToolChoiceMode::None)) => i::ToolChoice::Allowed(
            i::ToolChoiceAllowed::builder(
                i::ToolChoiceAllowedType::ToolChoiceAllowed,
                i::AllowedToolChoiceMode::Auto,
                vec![image_selector()],
            )
            .build(),
        ),
        Some(i::ToolChoice::Allowed(allowed))
            if image_only && allowed.mode == i::AllowedToolChoiceMode::Auto =>
        {
            i::ToolChoice::Hosted(
                i::ToolChoiceHosted::builder(i::ToolChoiceHostedType::ImageGeneration).build(),
            )
        }
        Some(i::ToolChoice::Allowed(mut allowed)) if !image_only => {
            if allowed.mode == i::AllowedToolChoiceMode::Auto {
                allowed.tools.push(image_selector());
            }
            i::ToolChoice::Allowed(allowed)
        }
        Some(i::ToolChoice::Mode(i::ToolChoiceMode::Required)) if !image_only => {
            let functions = (*out_tools)
                .iter()
                .flatten()
                .filter_map(|v| {
                    if let t::Tool::Function(v) = v {
                        Some(
                            serde_json::json!({"type":"function","name":v.name})
                                .as_object()
                                .unwrap()
                                .clone(),
                        )
                    } else {
                        None
                    }
                })
                .collect();
            i::ToolChoice::Allowed(
                i::ToolChoiceAllowed::builder(
                    i::ToolChoiceAllowedType::ToolChoiceAllowed,
                    i::AllowedToolChoiceMode::Required,
                    functions,
                )
                .build(),
            )
        }
        Some(choice @ i::ToolChoice::Function(_)) if !image_only => choice,
        _ => return Err(unsupported("image.tool_choice")),
    });
    (*out_tools)
        .get_or_insert_with(Vec::new)
        .push(t::Tool::ImageGeneration(tool));
    Ok(())
}
/// Remove the hosted image declaration from the function-only mapper while
/// preserving its selected modality/format separately in the concrete G config.
pub(crate) fn to_gemini(
    request_tools: &mut Option<Vec<t::Tool>>,
    request_choice: &mut Option<i::ToolChoice>,
    out_config: &mut Option<g::GenerationConfig>,
    report: &mut Report,
) -> Result<(), TransformError> {
    let mut image = None;
    let mut tools = Vec::new();
    for tool in (*request_tools).take().unwrap_or_default() {
        match tool {
            t::Tool::ImageGeneration(v) => {
                if image.replace(v).is_some() {
                    return Err(unsupported("tools.image_generation.count"));
                }
            }
            other => tools.push(other),
        }
    }
    let Some(image) = image else {
        if !tools.is_empty() {
            (*request_tools) = Some(tools);
        }
        return Ok(());
    };
    let mut enabled = true;
    let mut image_only = false;
    match (*request_choice).as_mut() {
        None | Some(i::ToolChoice::Mode(i::ToolChoiceMode::Auto)) => {}
        Some(i::ToolChoice::Mode(i::ToolChoiceMode::None)) | Some(i::ToolChoice::Function(_)) => {
            enabled = false
        }
        Some(i::ToolChoice::Mode(i::ToolChoiceMode::Required)) => {
            if !tools.is_empty() {
                return Err(unsupported("tool_choice.required_image_or_function"));
            }
            image_only = true;
            (*request_choice) = None;
        }
        Some(i::ToolChoice::Hosted(v)) if v.type_ == i::ToolChoiceHostedType::ImageGeneration => {
            image_only = true;
            tools.clear();
            (*request_choice) = None;
        }
        Some(i::ToolChoice::Allowed(allowed)) => {
            enabled = allowed
                .tools
                .iter()
                .any(|v| v.get("type").and_then(|v| v.as_str()) == Some("image_generation"));
            allowed
                .tools
                .retain(|v| v.get("type").and_then(|v| v.as_str()) != Some("image_generation"));
            if enabled && allowed.mode == i::AllowedToolChoiceMode::Required {
                if !allowed.tools.is_empty() {
                    return Err(unsupported("tool_choice.required_image_or_function"));
                }
                image_only = true;
                tools.clear();
                (*request_choice) = None;
            } else if enabled && allowed.tools.is_empty() {
                (*request_choice) = Some(i::ToolChoice::Mode(i::ToolChoiceMode::None));
            }
        }
        _ => return Err(unsupported("image.tool_choice")),
    }
    if enabled {
        for (bad, field) in [
            (
                image
                    .action
                    .as_ref()
                    .is_some_and(|v| *v != t::ImageAction::Auto),
                "image.action",
            ),
            (
                image
                    .background
                    .as_ref()
                    .is_some_and(|v| *v != t::ImageBackground::Auto),
                "image.background",
            ),
            (
                image
                    .quality
                    .as_ref()
                    .is_some_and(|v| *v != t::ImageQuality::Auto),
                "image.quality",
            ),
            (
                image.size.as_ref().is_some_and(|v| v != "auto"),
                "image.size",
            ),
            (
                image.partial_images.is_some_and(|v| v != 0),
                "image.partial_images",
            ),
            (
                image.input_fidelity.flatten().is_some(),
                "image.input_fidelity",
            ),
            (image.input_image_mask.is_some(), "image.mask"),
            (image.moderation.is_some(), "image.moderation"),
            (image.model.is_some(), "image.model"),
            (
                image.output_compression.is_some(),
                "image.output_compression",
            ),
        ] {
            if bad {
                return Err(unsupported(field));
            }
        }
        if image
            .output_format
            .as_ref()
            .is_some_and(|v| *v != t::ImageOutputFormat::Jpeg)
        {
            return Err(unsupported("image.output_format"));
        }
    } else {
        report.omitted(
            "tools.image_generation",
            "image tool is excluded by the actual tool selection",
        );
    }
    let config = (*out_config).get_or_insert_with(|| g::GenerationConfig::builder().build());
    config.response_modalities = Some(if !enabled {
        vec![g::Modality::Text]
    } else if image_only {
        vec![g::Modality::Image]
    } else {
        vec![g::Modality::Text, g::Modality::Image]
    });
    if enabled && image.output_format.is_some() {
        config.response_format = Some(
            g::ResponseFormatConfig::builder()
                .image(
                    g::ImageResponseFormat::builder()
                        .mime_type(g::ImageMimeType::ImageJpeg)
                        .delivery(g::Delivery::Inline)
                        .build(),
                )
                .build(),
        );
    }
    if tools.is_empty() {
        (*request_tools) = None;
        if matches!(
            *request_choice,
            Some(i::ToolChoice::Mode(i::ToolChoiceMode::Auto))
        ) {
            (*request_choice) = None;
        }
    } else {
        (*request_tools) = Some(tools);
    }
    Ok(())
}
