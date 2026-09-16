use crate::{
    transform::{Report, TransformError},
    wire::{
        gemini as g,
        openai::responses::{input as i, tools as t},
    },
};

pub(crate) fn to_responses(
    config: Option<&g::GenerationConfig>,
    out_tools: &mut Option<Vec<t::Tool>>,
    out_choice: &mut Option<i::ToolChoice>,
) -> Result<(), TransformError> {
    let Some(config) = config else {
        return Ok(());
    };

    let enabled = config
        .response_modalities
        .as_ref()
        .is_some_and(|v| v.contains(&g::Modality::Image));
    if !enabled {
        return Ok(());
    }

    let mut tool = t::ImageGenerationTool::builder().build();
    if let Some(format) = &config.response_format
        && let Some(image) = &format.image
        && image.mime_type == Some(g::ImageMimeType::ImageJpeg)
    {
        tool.output_format = Some(t::ImageOutputFormat::Jpeg);
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
        _ => i::ToolChoice::Mode(i::ToolChoiceMode::Auto),
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
                image = Some(v);
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
                image_only = true;
                tools.clear();
                (*request_choice) = None;
            } else if enabled && allowed.tools.is_empty() {
                (*request_choice) = Some(i::ToolChoice::Mode(i::ToolChoiceMode::None));
            }
        }
        _ => {
            *request_choice = None;
        }
    }
    if enabled {
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
    if enabled && image.output_format == Some(t::ImageOutputFormat::Jpeg) {
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
