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
    openai_models::Model {
        id,
        created,
        object: openai_models::ModelObject::Model,
        owned_by,
        rest: Default::default(),
    }
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
    report.omitted(
        "model.capabilities",
        "target dialect has no capability object",
    );
    report.omitted(
        "model.allowed_fallback_models",
        "target dialect has no fallback list",
    );
    report.omitted(
        "model.max_input_tokens",
        "target dialect has no input-token limit",
    );
    report.omitted(
        "model.max_tokens",
        "target dialect has no output-token limit",
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
        "model.display_name",
        "OpenAI model objects have no display name",
    );
    report.omitted(
        "model.description",
        "OpenAI model objects have no description",
    );
    report.omitted(
        "model.input_token_limit",
        "OpenAI model objects have no input limit",
    );
    report.omitted(
        "model.output_token_limit",
        "OpenAI model objects have no output limit",
    );
    report.omitted(
        "model.supported_generation_methods",
        "OpenAI model objects have no generation-method list",
    );
    report.omitted("model.thinking", "target dialect has no thinking field");
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
