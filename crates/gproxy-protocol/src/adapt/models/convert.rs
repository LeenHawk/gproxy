use super::{
    ModelDirectory, ModelListError, ModelListFailure, ModelListLimits, collect_claude,
    collect_gemini, collect_openai,
};
use crate::{
    WireRequest,
    capability::Upstream,
    transform::{
        Converted,
        models::{
            self, ClaudeModelSupplement, GeminiModelSupplement, ListPageFacts,
            OpenAiModelSupplement,
        },
    },
    wire::{claude::models as c, gemini::models as g, openai::models as o},
};
use std::collections::BTreeMap;

/// Collect the entire claude directory, then convert with actual per-model facts.
pub async fn claude_to_gemini_list<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,
    query: c::ListModelsQuery,
    supplements: &BTreeMap<String, GeminiModelSupplement>,
    limits: ModelListLimits,
) -> Result<ModelDirectory<Converted<g::ListModelsResponseBody>>, ModelListError> {
    let directory = collect_claude(upstream, target, request, query, limits).await?;
    let count = directory.value.data.len();
    let facts = ListPageFacts::complete();
    let value =
        models::claude_to_gemini_list(directory.value, supplements, &facts).map_err(|error| {
            ModelListError {
                completed_calls: directory.completed_calls,
                completed_models: count,
                failure: ModelListFailure::Transform(error),
            }
        })?;
    // A converted response may be larger than its native source representation.
    crate::codec::encode_json(&value.value, limits.codec).map_err(|error| ModelListError {
        completed_calls: directory.completed_calls,
        completed_models: count,
        failure: ModelListFailure::Transform(crate::transform::TransformError::new(
            crate::transform::TransformErrorKind::Limit,
            "models.response",
            error.to_string(),
        )),
    })?;
    Ok(ModelDirectory {
        value,
        completed_calls: directory.completed_calls,
    })
}

/// Collect the entire claude directory, then convert with actual per-model facts.
pub async fn claude_to_openai_list<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,
    query: c::ListModelsQuery,
    supplements: &BTreeMap<String, OpenAiModelSupplement>,
    limits: ModelListLimits,
) -> Result<ModelDirectory<Converted<o::ListModelsResponseBody>>, ModelListError> {
    let directory = collect_claude(upstream, target, request, query, limits).await?;
    let count = directory.value.data.len();
    let facts = ListPageFacts::complete();
    let value =
        models::claude_to_openai_list(directory.value, supplements, &facts).map_err(|error| {
            ModelListError {
                completed_calls: directory.completed_calls,
                completed_models: count,
                failure: ModelListFailure::Transform(error),
            }
        })?;
    // A converted response may be larger than its native source representation.
    crate::codec::encode_json(&value.value, limits.codec).map_err(|error| ModelListError {
        completed_calls: directory.completed_calls,
        completed_models: count,
        failure: ModelListFailure::Transform(crate::transform::TransformError::new(
            crate::transform::TransformErrorKind::Limit,
            "models.response",
            error.to_string(),
        )),
    })?;
    Ok(ModelDirectory {
        value,
        completed_calls: directory.completed_calls,
    })
}

/// Collect the entire gemini directory, then convert with actual per-model facts.
pub async fn gemini_to_claude_list<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,
    query: g::ListModelsQuery,
    supplements: &BTreeMap<String, ClaudeModelSupplement>,
    empty_page_ids: Option<(String, String)>,
    limits: ModelListLimits,
) -> Result<ModelDirectory<Converted<c::ListModelsResponseBody>>, ModelListError> {
    let directory = collect_gemini(upstream, target, request, query, limits).await?;
    let count = directory.value.models.as_ref().map_or(0, Vec::len);
    let mut facts = ListPageFacts::complete();
    if count == 0
        && let Some((first, last)) = empty_page_ids
    {
        facts.first_id = Some(first);
        facts.last_id = Some(last);
    }
    let value =
        models::gemini_to_claude_list(directory.value, supplements, &facts).map_err(|error| {
            ModelListError {
                completed_calls: directory.completed_calls,
                completed_models: count,
                failure: ModelListFailure::Transform(error),
            }
        })?;
    // A converted response may be larger than its native source representation.
    crate::codec::encode_json(&value.value, limits.codec).map_err(|error| ModelListError {
        completed_calls: directory.completed_calls,
        completed_models: count,
        failure: ModelListFailure::Transform(crate::transform::TransformError::new(
            crate::transform::TransformErrorKind::Limit,
            "models.response",
            error.to_string(),
        )),
    })?;
    Ok(ModelDirectory {
        value,
        completed_calls: directory.completed_calls,
    })
}

/// Collect the entire gemini directory, then convert with actual per-model facts.
pub async fn gemini_to_openai_list<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,
    query: g::ListModelsQuery,
    supplements: &BTreeMap<String, OpenAiModelSupplement>,
    limits: ModelListLimits,
) -> Result<ModelDirectory<Converted<o::ListModelsResponseBody>>, ModelListError> {
    let directory = collect_gemini(upstream, target, request, query, limits).await?;
    let count = directory.value.models.as_ref().map_or(0, Vec::len);
    let facts = ListPageFacts::complete();
    let value =
        models::gemini_to_openai_list(directory.value, supplements, &facts).map_err(|error| {
            ModelListError {
                completed_calls: directory.completed_calls,
                completed_models: count,
                failure: ModelListFailure::Transform(error),
            }
        })?;
    // A converted response may be larger than its native source representation.
    crate::codec::encode_json(&value.value, limits.codec).map_err(|error| ModelListError {
        completed_calls: directory.completed_calls,
        completed_models: count,
        failure: ModelListFailure::Transform(crate::transform::TransformError::new(
            crate::transform::TransformErrorKind::Limit,
            "models.response",
            error.to_string(),
        )),
    })?;
    Ok(ModelDirectory {
        value,
        completed_calls: directory.completed_calls,
    })
}

/// Collect the entire openai directory, then convert with actual per-model facts.
pub async fn openai_to_claude_list<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,

    supplements: &BTreeMap<String, ClaudeModelSupplement>,
    empty_page_ids: Option<(String, String)>,
    limits: ModelListLimits,
) -> Result<ModelDirectory<Converted<c::ListModelsResponseBody>>, ModelListError> {
    let directory = collect_openai(upstream, target, request, limits).await?;
    let count = directory.value.data.len();
    let mut facts = ListPageFacts::complete();
    if count == 0
        && let Some((first, last)) = empty_page_ids
    {
        facts.first_id = Some(first);
        facts.last_id = Some(last);
    }
    let value =
        models::openai_to_claude_list(directory.value, supplements, &facts).map_err(|error| {
            ModelListError {
                completed_calls: directory.completed_calls,
                completed_models: count,
                failure: ModelListFailure::Transform(error),
            }
        })?;
    // A converted response may be larger than its native source representation.
    crate::codec::encode_json(&value.value, limits.codec).map_err(|error| ModelListError {
        completed_calls: directory.completed_calls,
        completed_models: count,
        failure: ModelListFailure::Transform(crate::transform::TransformError::new(
            crate::transform::TransformErrorKind::Limit,
            "models.response",
            error.to_string(),
        )),
    })?;
    Ok(ModelDirectory {
        value,
        completed_calls: directory.completed_calls,
    })
}

/// Collect the entire openai directory, then convert with actual per-model facts.
pub async fn openai_to_gemini_list<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,

    supplements: &BTreeMap<String, GeminiModelSupplement>,
    limits: ModelListLimits,
) -> Result<ModelDirectory<Converted<g::ListModelsResponseBody>>, ModelListError> {
    let directory = collect_openai(upstream, target, request, limits).await?;
    let count = directory.value.data.len();
    let facts = ListPageFacts::complete();
    let value =
        models::openai_to_gemini_list(directory.value, supplements, &facts).map_err(|error| {
            ModelListError {
                completed_calls: directory.completed_calls,
                completed_models: count,
                failure: ModelListFailure::Transform(error),
            }
        })?;
    // A converted response may be larger than its native source representation.
    crate::codec::encode_json(&value.value, limits.codec).map_err(|error| ModelListError {
        completed_calls: directory.completed_calls,
        completed_models: count,
        failure: ModelListFailure::Transform(crate::transform::TransformError::new(
            crate::transform::TransformErrorKind::Limit,
            "models.response",
            error.to_string(),
        )),
    })?;
    Ok(ModelDirectory {
        value,
        completed_calls: directory.completed_calls,
    })
}
