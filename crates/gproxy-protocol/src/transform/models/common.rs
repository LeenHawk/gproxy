use super::supplement::{ClaudeModelSupplement, Converted};
use crate::transform::{Report, TransformError};
use crate::wire::{claude::models as claude_models, openai::models as openai_models};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(crate) fn converted<T>(value: T, report: Report) -> Converted<T> {
    Converted { value, report }
}

pub(crate) fn require<T>(value: Option<T>, field: &str) -> Result<T, TransformError> {
    value.ok_or_else(|| TransformError::missing_metadata(field))
}

pub(crate) fn non_empty(value: &str, field: &str) -> Result<String, TransformError> {
    if value.is_empty() {
        Err(TransformError::missing_metadata(field))
    } else {
        Ok(value.to_owned())
    }
}

pub(crate) fn choose<T: PartialEq>(
    source: Option<T>,
    supplement: Option<T>,
    field: &str,
) -> Result<T, TransformError> {
    match (source, supplement) {
        (Some(source), Some(supplement)) if source != supplement => Err(TransformError::new(
            crate::transform::TransformErrorKind::Conflict,
            field,
            "source and supplied metadata disagree",
        )),
        (Some(source), _) => Ok(source),
        (None, Some(supplement)) => Ok(supplement),
        (None, None) => Err(TransformError::missing_metadata(field)),
    }
}

pub(crate) fn choose_optional<T: PartialEq>(
    source: Option<T>,
    supplement: Option<T>,
    field: &str,
) -> Result<Option<T>, TransformError> {
    match (source, supplement) {
        (Some(source), Some(supplement)) if source != supplement => Err(TransformError::new(
            crate::transform::TransformErrorKind::Conflict,
            field,
            "source model representations disagree",
        )),
        (Some(source), _) => Ok(Some(source)),
        (None, supplement) => Ok(supplement),
    }
}

pub(crate) fn openai_identity(model: &openai_models::Model) -> Result<&str, TransformError> {
    if !model.id.is_empty() {
        return Ok(&model.id);
    }
    model
        .slug
        .as_deref()
        .filter(|slug| !slug.is_empty())
        .ok_or_else(|| TransformError::shape("model.id", "model id and slug are empty"))
}

pub(crate) fn openai_thinking(model: &openai_models::Model) -> Option<bool> {
    model.thinking_supported.or_else(|| {
        model
            .supported_reasoning_levels
            .as_ref()
            .and_then(|levels| {
                levels
                    .iter()
                    .any(|level| level.effort != openai_models::ModelReasoningEffort::None)
                    .then_some(true)
            })
    })
}

pub(crate) fn iso_to_epoch_with_loss(value: &str) -> Result<(i64, bool), TransformError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|date| {
            (
                date.unix_timestamp(),
                date.unix_timestamp_nanos() % 1_000_000_000 != 0,
            )
        })
        .map_err(|error| {
            TransformError::shape(
                "model.created_at",
                format!("invalid RFC3339 timestamp: {error}"),
            )
        })
}

pub(crate) fn parse_iso(value: &str) -> Result<OffsetDateTime, TransformError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| {
        TransformError::shape(
            "model.created_at",
            format!("invalid RFC3339 timestamp: {error}"),
        )
    })
}

pub(crate) fn epoch_to_iso(value: i64) -> Result<String, TransformError> {
    OffsetDateTime::from_unix_timestamp(value)
        .map_err(|error| TransformError::shape("model.created", error.to_string()))?
        .format(&Rfc3339)
        .map_err(|error| TransformError::shape("model.created", error.to_string()))
}

pub(crate) fn openai_model_id(value: &str) -> Result<String, TransformError> {
    if value.is_empty() {
        Err(TransformError::shape("model.id", "model id is empty"))
    } else {
        Ok(value.to_owned())
    }
}

pub(crate) fn gemini_name(value: &str) -> Result<String, TransformError> {
    if value.is_empty() {
        return Err(TransformError::shape("model.name", "model id is empty"));
    }
    if value.starts_with("models/") {
        if value.len() == "models/".len() {
            return Err(TransformError::shape("model.name", "empty models resource"));
        }
        Ok(value.to_owned())
    } else {
        Ok(format!("models/{value}"))
    }
}

pub(crate) fn bare_gemini_id(value: &str) -> Result<String, TransformError> {
    let Some(id) = value.strip_prefix("models/") else {
        return Err(TransformError::shape(
            "model.name",
            "Gemini model resource must begin with models/",
        ));
    };
    if id.is_empty() {
        Err(TransformError::shape("model.name", "empty models resource"))
    } else {
        Ok(id.to_owned())
    }
}

pub(crate) fn openai_model(id: String, created: i64, owned_by: String) -> openai_models::Model {
    openai_models::Model::builder(
        id,
        Some(created),
        Some(openai_models::ModelObject::Model),
        Some(owned_by),
    )
    .build()
}

pub(crate) fn claude_model(
    id: String,
    created_at: String,
    display_name: String,
    max_input_tokens: i64,
    max_tokens: i64,
    supplement: ClaudeModelSupplement,
) -> claude_models::ModelInfo {
    claude_models::ModelInfo {
        id,
        allowed_fallback_models: supplement.allowed_fallback_models,
        capabilities: supplement.capabilities.into_wire(),
        created_at,
        display_name,
        max_input_tokens,
        max_tokens,
        type_: claude_models::ModelType::Model,
        rest: Default::default(),
    }
}

pub(crate) fn merge_report(target: &mut Report, source: Report) {
    target.diagnostics.extend(source.diagnostics);
}

pub(crate) fn report_openai_loss(report: &mut Report) {
    report.omitted("model.owned_by", "target dialect has no owner field");
}

pub(crate) fn report_claude_loss(report: &mut Report) {
    for field in [
        "batch",
        "citations",
        "code_execution",
        "context_management",
        "pdf_input",
        "structured_outputs",
        "thinking.types",
    ] {
        report.omitted(
            format!("model.capabilities.{field}"),
            "target has no corresponding Claude capability field",
        );
    }
    report.omitted(
        "model.allowed_fallback_models",
        "target dialect has no fallback list",
    );
}

pub(crate) fn report_gemini_loss(report: &mut Report) {
    report.omitted(
        "model.base_model_id",
        "OpenAI model objects have no base-model id",
    );
    report.omitted(
        "model.version",
        "OpenAI model objects have no model version",
    );
    report.omitted(
        "model.temperature",
        "OpenAI model objects have no temperature field",
    );
    report.omitted(
        "model.max_temperature",
        "OpenAI model objects have no max_temperature field",
    );
    report.omitted("model.top_p", "OpenAI model objects have no top_p field");
    report.omitted("model.top_k", "OpenAI model objects have no top_k field");
}

pub(crate) fn report_openai_extensions(model: &openai_models::Model, report: &mut Report) {
    macro_rules! omit {
        ($($field:ident),* $(,)?) => {$(
            if model.$field.is_some() {
                report.omitted(concat!("model.", stringify!($field)),
                    "target model schema has no corresponding catalog field");
            }
        )*};
    }
    omit!(
        instructions,
        output_modalities,
        supported_parameters,
        default_reasoning_level,
        service_tiers,
        default_service_tier,
        supported_actions,
        guardian,
        shell_type,
        visibility,
        supported_in_api,
        priority,
        additional_speed_tiers,
        available_access_programs,
        availability_nux,
        upgrade,
        model_messages,
        include_skills_usage_instructions,
        include_plugin_usage_instructions,
        include_apps_usage_instructions,
        supports_reasoning_summary_parameter,
        default_reasoning_summary,
        support_verbosity,
        default_verbosity,
        apply_patch_tool_type,
        web_search_tool_type,
        truncation_policy,
        supports_image_detail_original,
        auto_compact_token_limit,
        comp_hash,
        effective_context_window_percent,
        experimental_supported_tools,
        supports_search_tool,
        supports_experimental_context,
        use_responses_lite,
        supports_reasoning_effort_updates,
        node_repl_auto_review_required,
        node_repl_disabled,
        auto_review_model_override,
        model_specialty,
        tool_mode,
        multi_agent_version,
        multi_agent_reasoning_effort,
        base_instructions,
        prefer_websockets,
        supports_parallel_tool_calls,
        supports_reasoning_summaries,
        requires_sandboxed_review,
        available_in_plans,
        minimal_client_version,
    );
    if model.id.is_empty() && model.slug.is_some() {
        report.changed("model.slug", "slug used as target model identity");
    } else if let Some(slug) = &model.slug
        && slug != &model.id
    {
        report.omitted("model.slug", "target has only one model identity");
    }
    if model.max_context_window.is_some() {
        if model.context_window.is_none() {
            report.changed(
                "model.max_context_window",
                "maximum context window used as input token limit",
            );
        } else {
            report.omitted(
                "model.max_context_window",
                "target cannot distinguish the default window from its override ceiling",
            );
        }
    }
}

pub(crate) fn reconcile_openai_capabilities(
    model: &openai_models::Model,
    mut supplement: super::supplement::ClaudeModelSupplement,
) -> Result<super::supplement::ClaudeModelSupplement, TransformError> {
    supplement.capabilities.thinking.supported = choose(
        openai_thinking(model),
        Some(supplement.capabilities.thinking.supported),
        "model.thinking_supported",
    )?;
    if let Some(modalities) = &model.input_modalities {
        supplement.capabilities.image_input = choose(
            Some(modalities.iter().any(|modality| modality == "image")),
            Some(supplement.capabilities.image_input),
            "model.input_modalities.image",
        )?;
    }
    if let Some(levels) = &model.supported_reasoning_levels {
        use openai_models::ModelReasoningEffort as Effort;
        let has = |effort| levels.iter().any(|level| level.effort == effort);
        macro_rules! reconcile {
            ($($field:ident => $effort:ident),* $(,)?) => {$(
                supplement.capabilities.effort.$field = choose(
                    Some(has(Effort::$effort)), Some(supplement.capabilities.effort.$field),
                    concat!("model.supported_reasoning_levels.", stringify!($field)),
                )?;
            )*};
        }
        reconcile!(low => Low, medium => Medium, high => High, xhigh => Xhigh, max => Max);
        supplement.capabilities.effort.supported = choose(
            Some(
                [
                    Effort::Low,
                    Effort::Medium,
                    Effort::High,
                    Effort::Xhigh,
                    Effort::Max,
                ]
                .into_iter()
                .any(has),
            ),
            Some(supplement.capabilities.effort.supported),
            "model.supported_reasoning_levels",
        )?;
    }
    Ok(supplement)
}
