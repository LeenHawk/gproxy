use super::{
    common,
    supplement::{ClaudeModelSupplement, Converted, GeminiModelSupplement, OpenAiModelSupplement},
};
use crate::transform::{Report, TransformError};
use crate::wire::{
    claude::models as claude_models, gemini::models as gemini_models,
    openai::models as openai_models,
};

pub fn claude_to_openai(
    input: claude_models::ModelInfo,
    supplement: &OpenAiModelSupplement,
) -> Result<Converted<openai_models::Model>, TransformError> {
    let (source_created, fractional) = common::iso_to_epoch_with_loss(&input.created_at)?;
    if fractional && supplement.created.is_some() {
        return Err(TransformError::new(
            crate::transform::TransformErrorKind::Conflict,
            "model.created",
            "fractional source timestamp cannot equal integer supplied metadata",
        ));
    }
    let created = common::choose(Some(source_created), supplement.created, "model.created")?;
    if supplement.owned_by.is_empty() {
        return Err(TransformError::missing_metadata("model.owned_by"));
    }
    let mut report = Report::default();
    if fractional {
        report.changed(
            "model.created_at",
            "OpenAI model timestamps have whole-second precision",
        );
    }
    common::report_claude_loss(&mut report);
    let mut value = common::openai_model(
        common::openai_model_id(&input.id)?,
        created,
        supplement.owned_by.clone(),
    );
    value.display_name = Some(input.display_name);
    value.context_window = Some(
        u64::try_from(input.max_input_tokens)
            .map_err(|_| TransformError::shape("model.max_input_tokens", "negative limit"))?,
    );
    value.max_output_tokens = Some(
        u64::try_from(input.max_tokens)
            .map_err(|_| TransformError::shape("model.max_tokens", "negative limit"))?,
    );
    value.thinking_supported = Some(input.capabilities.thinking.supported);
    let mut modalities = vec!["text".to_owned()];
    if input.capabilities.image_input.supported {
        modalities.push("image".to_owned());
    }
    value.input_modalities = Some(modalities);
    let effort = &input.capabilities.effort;
    use openai_models::ModelReasoningEffort as Effort;
    value.supported_reasoning_levels = Some(
        [
            (Effort::Low, effort.low.supported),
            (Effort::Medium, effort.medium.supported),
            (Effort::High, effort.high.supported),
            (Effort::Xhigh, effort.xhigh.supported),
            (Effort::Max, effort.max.supported),
        ]
        .into_iter()
        .filter(|(_, supported)| effort.supported && *supported)
        .map(|(effort, _)| openai_models::ModelReasoningLevel {
            effort,
            description: String::new(),
        })
        .collect(),
    );
    Ok(common::converted(value, report))
}

pub fn claude_to_gemini(
    input: claude_models::ModelInfo,
    supplement: &GeminiModelSupplement,
) -> Result<Converted<gemini_models::Model>, TransformError> {
    let name = common::gemini_name(&input.id)?;
    let base_model_id = common::non_empty(&supplement.base_model_id, "model.base_model_id")?;
    let version = common::non_empty(&supplement.version, "model.version")?;
    let mut report = Report::default();
    report.omitted(
        "model.capabilities.batch",
        "Gemini has no Claude batch capability",
    );
    report.omitted(
        "model.capabilities.citations",
        "Gemini has no Claude citations capability",
    );
    report.omitted(
        "model.capabilities.code_execution",
        "Gemini has no Claude code-execution capability",
    );
    report.omitted(
        "model.capabilities.context_management",
        "Gemini has no Claude context-management capability",
    );
    report.omitted(
        "model.capabilities.effort",
        "Gemini has no Claude effort capability",
    );
    report.omitted(
        "model.capabilities.image_input",
        "Gemini has no Claude image-input capability",
    );
    report.omitted(
        "model.capabilities.pdf_input",
        "Gemini has no Claude PDF-input capability",
    );
    report.omitted(
        "model.capabilities.structured_outputs",
        "Gemini has no Claude structured-output capability",
    );
    report.omitted(
        "model.capabilities.thinking.types",
        "Gemini represents thinking support but not Claude thinking modes",
    );
    report.omitted(
        "model.allowed_fallback_models",
        "Gemini has no fallback-model list",
    );
    if input.line.is_some() {
        report.omitted("model.line", "Gemini has no model line field");
    }
    if input.capabilities.server_tools.is_some() {
        report.omitted(
            "model.capabilities.server_tools",
            "Gemini has no server-tool capability fields",
        );
    }
    report.omitted("model.created_at", "Gemini has no model creation timestamp");
    report.omitted("model.type", "Gemini has no model type field");
    let value = gemini_models::Model {
        name,
        base_model_id,
        version,
        display_name: Some(input.display_name),
        description: None,
        input_token_limit: Some(input.max_input_tokens),
        output_token_limit: Some(input.max_tokens),
        supported_generation_methods: None,
        thinking: Some(input.capabilities.thinking.supported),
        temperature: None,
        max_temperature: None,
        top_p: None,
        top_k: None,
        rest: Default::default(),
    };
    Ok(common::converted(value, report))
}

pub fn openai_to_claude(
    input: openai_models::Model,
    supplement: &ClaudeModelSupplement,
) -> Result<Converted<claude_models::ModelInfo>, TransformError> {
    let identity = common::openai_identity(&input)?.to_owned();
    let created_at = match input.created {
        Some(created) => {
            if let Some(supplied) = &supplement.created_at
                && common::parse_iso(supplied)?.unix_timestamp_nanos()
                    != created as i128 * 1_000_000_000
            {
                return Err(TransformError::new(
                    crate::transform::TransformErrorKind::Conflict,
                    "model.created_at",
                    "source and supplied metadata disagree",
                ));
            }
            common::epoch_to_iso(created)?
        }
        None => {
            let supplied = common::require(supplement.created_at.clone(), "model.created_at")?;
            common::parse_iso(&supplied)?;
            supplied
        }
    };
    let display_name = common::choose(
        input.display_name.clone(),
        supplement.display_name.clone(),
        "model.display_name",
    )?;
    let source_input = input
        .context_window
        .or(input.max_context_window)
        .map(i64::try_from)
        .transpose()
        .map_err(|e| TransformError::shape("model.context_window", e.to_string()))?;
    let source_output = input
        .max_output_tokens
        .map(i64::try_from)
        .transpose()
        .map_err(|e| TransformError::shape("model.max_output_tokens", e.to_string()))?;
    let max_input_tokens = common::choose(
        source_input,
        supplement.max_input_tokens,
        "model.max_input_tokens",
    )?;
    let max_tokens = common::choose(source_output, supplement.max_tokens, "model.max_tokens")?;
    let mut report = Report::default();
    common::report_openai_extensions(&input, &mut report);
    if input.description.is_some() {
        report.omitted(
            "model.description",
            "Claude model objects have no description",
        );
    }
    if input.generation_methods.is_some() {
        report.omitted(
            "model.generation_methods",
            "Claude model objects have no generation-method list",
        );
    }
    if let Some(modalities) = &input.input_modalities
        && modalities
            .iter()
            .any(|modality| !matches!(modality.as_str(), "text" | "image"))
    {
        report.omitted(
            "model.input_modalities",
            "Claude capabilities only represent image input from this modality list",
        );
    }
    if let Some(levels) = &input.supported_reasoning_levels
        && levels.iter().any(|level| {
            !level.description.is_empty()
                || !matches!(
                    level.effort,
                    openai_models::ModelReasoningEffort::None
                        | openai_models::ModelReasoningEffort::Low
                        | openai_models::ModelReasoningEffort::Medium
                        | openai_models::ModelReasoningEffort::High
                        | openai_models::ModelReasoningEffort::Xhigh
                        | openai_models::ModelReasoningEffort::Max
                )
        })
    {
        report.omitted(
            "model.supported_reasoning_levels",
            "effort descriptions and some choices have no Claude capability field",
        );
    }
    let supplement = common::reconcile_openai_capabilities(&input, supplement.clone())?;

    common::report_openai_loss(&mut report);
    report.omitted(
        "model.object",
        "Claude model objects have a distinct type field",
    );
    Ok(common::converted(
        common::claude_model(
            identity,
            created_at,
            display_name,
            max_input_tokens,
            max_tokens,
            supplement,
        ),
        report,
    ))
}

pub fn openai_to_gemini(
    input: openai_models::Model,
    supplement: &GeminiModelSupplement,
) -> Result<Converted<gemini_models::Model>, TransformError> {
    let identity = common::openai_identity(&input)?.to_owned();
    let mut report = Report::default();
    common::report_openai_extensions(&input, &mut report);
    if input.input_modalities.is_some() {
        report.omitted(
            "model.input_modalities",
            "Gemini model objects have no input-modality list",
        );
    }
    if input.supported_reasoning_levels.is_some() {
        report.omitted(
            "model.supported_reasoning_levels",
            "Gemini model objects only advertise a thinking boolean",
        );
    }
    let thinking = common::openai_thinking(&input);

    report.omitted("model.owned_by", "Gemini has no owner field");
    report.omitted("model.object", "Gemini has no OpenAI object discriminator");
    report.omitted("model.created", "Gemini has no model creation timestamp");
    let base_model_id = common::non_empty(&supplement.base_model_id, "model.base_model_id")?;
    let version = common::non_empty(&supplement.version, "model.version")?;
    let value = gemini_models::Model {
        name: common::gemini_name(&identity)?,
        base_model_id,
        version,
        display_name: input.display_name,
        description: input.description,
        input_token_limit: input
            .context_window
            .or(input.max_context_window)
            .map(i64::try_from)
            .transpose()
            .map_err(|e| TransformError::shape("model.context_window", e.to_string()))?,
        output_token_limit: input
            .max_output_tokens
            .map(i64::try_from)
            .transpose()
            .map_err(|e| TransformError::shape("model.max_output_tokens", e.to_string()))?,
        supported_generation_methods: input.generation_methods,
        thinking,
        temperature: None,
        max_temperature: None,
        top_p: None,
        top_k: None,
        rest: Default::default(),
    };
    Ok(common::converted(value, report))
}

pub fn gemini_to_openai(
    input: gemini_models::Model,
    supplement: &OpenAiModelSupplement,
) -> Result<Converted<openai_models::Model>, TransformError> {
    let created = common::require(supplement.created, "model.created")?;
    if supplement.owned_by.is_empty() {
        return Err(TransformError::missing_metadata("model.owned_by"));
    }
    let mut report = Report::default();
    common::report_gemini_loss(&mut report);
    let mut value = common::openai_model(
        common::bare_gemini_id(&input.name)?,
        created,
        supplement.owned_by.clone(),
    );
    value.display_name = input.display_name;
    value.description = input.description;
    value.context_window = input
        .input_token_limit
        .map(u64::try_from)
        .transpose()
        .map_err(|e| TransformError::shape("model.input_token_limit", e.to_string()))?;
    value.max_output_tokens = input
        .output_token_limit
        .map(u64::try_from)
        .transpose()
        .map_err(|e| TransformError::shape("model.output_token_limit", e.to_string()))?;
    value.thinking_supported = input.thinking;
    value.generation_methods = input.supported_generation_methods;
    Ok(common::converted(value, report))
}

pub fn gemini_to_claude(
    input: gemini_models::Model,
    supplement: &ClaudeModelSupplement,
) -> Result<Converted<claude_models::ModelInfo>, TransformError> {
    let created_at = common::require(supplement.created_at.clone(), "model.created_at")?;
    common::parse_iso(&created_at)?;
    let display_name = common::choose(
        input.display_name.clone(),
        supplement.display_name.clone(),
        "model.display_name",
    )?;
    let max_input_tokens = common::choose(
        input.input_token_limit,
        supplement.max_input_tokens,
        "model.max_input_tokens",
    )?;
    let max_tokens = common::choose(
        input.output_token_limit,
        supplement.max_tokens,
        "model.max_tokens",
    )?;
    let mut report = Report::default();
    report.omitted(
        "model.description",
        "Claude model objects have no description",
    );
    report.omitted(
        "model.supported_generation_methods",
        "Claude model objects have no generation-method list",
    );
    report.omitted(
        "model.base_model_id",
        "Claude model objects have no base-model id",
    );
    report.omitted(
        "model.version",
        "Claude model objects have no model version",
    );
    report.omitted(
        "model.temperature",
        "Claude model objects have no temperature field",
    );
    report.omitted(
        "model.max_temperature",
        "Claude model objects have no max_temperature field",
    );
    report.omitted("model.top_p", "Claude model objects have no top_p field");
    report.omitted("model.top_k", "Claude model objects have no top_k field");
    if input.thinking.is_some() {
        report.changed(
            "model.thinking",
            "Claude thinking support was reconciled with source metadata",
        );
    }
    let mut supplement = supplement.clone();
    supplement.capabilities.thinking.supported = common::choose(
        input.thinking,
        Some(supplement.capabilities.thinking.supported),
        "model.thinking",
    )?;
    Ok(common::converted(
        common::claude_model(
            common::bare_gemini_id(&input.name)?,
            created_at,
            display_name,
            max_input_tokens,
            max_tokens,
            supplement,
        ),
        report,
    ))
}
