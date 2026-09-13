use super::{
    common, get,
    supplement::{
        ClaudeModelSupplement, Converted, GeminiModelSupplement, ListPageFacts,
        OpenAiModelSupplement, missing_pagination,
    },
};
use crate::transform::{Report, TransformError};
use crate::wire::{
    claude::models as claude_models, gemini::models as gemini_models,
    openai::models as openai_models,
};
use std::collections::{BTreeMap, BTreeSet};

/// List supplement maps use the exact source resource field: `id` for
/// Claude/OpenAI and `name` (including its `models/` prefix) for Gemini.
fn supplement_for<'a, T>(
    supplements: &'a BTreeMap<String, T>,
    source_key: &str,
) -> Result<&'a T, TransformError> {
    supplements
        .get(source_key)
        .ok_or_else(|| TransformError::missing_metadata(format!("model[{source_key}].supplement")))
}

fn require_complete(page: &ListPageFacts) -> Result<(), TransformError> {
    validate_facts(page)?;
    if page.complete {
        Ok(())
    } else {
        Err(missing_pagination(
            "complete directory facts are required (NeedsPagination)",
        ))
    }
}

fn require_complete_without_target_pagination(
    page: &ListPageFacts,
    source_has_more: bool,
    source_has_next_token: bool,
) -> Result<(), TransformError> {
    if source_has_more || source_has_next_token {
        return Err(missing_pagination(
            "source continuation cannot be represented by target list envelope (NeedsPagination)",
        ));
    }
    require_complete(page)
}

fn validate_facts(page: &ListPageFacts) -> Result<(), TransformError> {
    if page.complete && (page.has_more == Some(true) || page.next_page_token.is_some()) {
        return Err(TransformError::new(
            crate::transform::TransformErrorKind::Conflict,
            "pagination.complete",
            "complete directory cannot have a continuation",
        ));
    }
    if page.next_page_token.as_ref().is_some_and(String::is_empty) {
        return Err(TransformError::shape(
            "pagination.next_page_token",
            "empty page token",
        ));
    }
    Ok(())
}

fn validate_source_page(
    page: &ListPageFacts,
    source_has_more: bool,
    target_is_gemini: bool,
) -> Result<(), TransformError> {
    validate_facts(page)?;
    if source_has_more && page.complete {
        return Err(TransformError::new(
            crate::transform::TransformErrorKind::Conflict,
            "pagination.complete",
            "source explicitly has another page",
        ));
    }
    let target_has_more = if target_is_gemini {
        page.next_page_token.is_some()
    } else {
        page.has_more.unwrap_or(false)
    };
    if source_has_more != target_has_more {
        return Err(missing_pagination(
            "source continuation requires matching target page facts (NeedsPagination)",
        ));
    }
    if !page.complete && !target_is_gemini && page.has_more.is_none() {
        return Err(missing_pagination(
            "Claude target requires explicit has_more",
        ));
    }
    Ok(())
}

fn unique_ids<'a>(ids: impl Iterator<Item = &'a str>) -> Result<(), TransformError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(TransformError::new(
                crate::transform::TransformErrorKind::Conflict,
                "model.id",
                format!("duplicate converted model identity: {id}"),
            ));
        }
    }
    Ok(())
}

pub fn claude_to_openai_list(
    input: claude_models::ListModelsResponseBody,
    supplements: &BTreeMap<String, OpenAiModelSupplement>,
    page: &ListPageFacts,
) -> Result<Converted<openai_models::ListModelsResponseBody>, TransformError> {
    require_complete_without_target_pagination(page, input.has_more, false)?;
    let mut report = Report::default();
    let mut data = Vec::with_capacity(input.data.len());
    for model in input.data {
        let supplement = supplement_for(supplements, &model.id)?;
        let converted = get::claude_to_openai(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        data.push(converted.value);
    }
    unique_ids(data.iter().map(|model| model.id.as_str()))?;
    report.omitted("pagination", "OpenAI model lists have no cursor envelope");
    Ok(common::converted(
        openai_models::ListModelsResponseBody {
            data,
            object: openai_models::ListObject::List,
            rest: Default::default(),
        },
        report,
    ))
}

pub fn claude_to_gemini_list(
    input: claude_models::ListModelsResponseBody,
    supplements: &BTreeMap<String, GeminiModelSupplement>,
    page: &ListPageFacts,
) -> Result<Converted<gemini_models::ListModelsResponseBody>, TransformError> {
    validate_source_page(page, input.has_more, true)?;
    let mut report = Report::default();
    let mut models = Vec::with_capacity(input.data.len());
    for model in input.data {
        let supplement = supplement_for(supplements, &model.id)?;
        let converted = get::claude_to_gemini(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        models.push(converted.value);
    }
    unique_ids(models.iter().map(|model| model.name.as_str()))?;
    Ok(common::converted(
        gemini_models::ListModelsResponseBody {
            models: Some(models),
            next_page_token: page.next_page_token.clone(),
            rest: Default::default(),
        },
        report,
    ))
}

pub fn openai_to_claude_list(
    input: openai_models::ListModelsResponseBody,
    supplements: &BTreeMap<String, ClaudeModelSupplement>,
    page: &ListPageFacts,
) -> Result<Converted<claude_models::ListModelsResponseBody>, TransformError> {
    require_complete(page)?;
    let mut report = Report::default();
    let mut data = Vec::with_capacity(input.data.len());
    for model in input.data {
        let supplement = supplement_for(supplements, &model.id)?;
        let converted = get::openai_to_claude(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        data.push(converted.value);
    }
    unique_ids(data.iter().map(|model| model.id.as_str()))?;
    let (first_id, last_id, has_more) = claude_page(&data, page)?;
    Ok(common::converted(
        claude_models::ListModelsResponseBody {
            first_id,
            last_id,
            has_more,
            data,
            rest: Default::default(),
        },
        report,
    ))
}

pub fn openai_to_gemini_list(
    input: openai_models::ListModelsResponseBody,
    supplements: &BTreeMap<String, GeminiModelSupplement>,
    page: &ListPageFacts,
) -> Result<Converted<gemini_models::ListModelsResponseBody>, TransformError> {
    require_complete(page)?;
    let mut report = Report::default();
    let mut models = Vec::with_capacity(input.data.len());
    for model in input.data {
        let supplement = supplement_for(supplements, &model.id)?;
        let converted = get::openai_to_gemini(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        models.push(converted.value);
    }
    unique_ids(models.iter().map(|model| model.name.as_str()))?;
    Ok(common::converted(
        gemini_models::ListModelsResponseBody {
            models: Some(models),
            next_page_token: page.next_page_token.clone(),
            rest: Default::default(),
        },
        report,
    ))
}

pub fn gemini_to_openai_list(
    input: gemini_models::ListModelsResponseBody,
    supplements: &BTreeMap<String, OpenAiModelSupplement>,
    page: &ListPageFacts,
) -> Result<Converted<openai_models::ListModelsResponseBody>, TransformError> {
    require_complete_without_target_pagination(page, false, input.next_page_token.is_some())?;
    let mut report = Report::default();
    let source_models = input.models.unwrap_or_default();
    let mut data = Vec::with_capacity(source_models.len());
    for model in source_models {
        let supplement = supplement_for(supplements, &model.name)?;
        let converted = get::gemini_to_openai(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        data.push(converted.value);
    }
    unique_ids(data.iter().map(|model| model.id.as_str()))?;
    report.omitted("pagination", "OpenAI model lists have no cursor envelope");
    Ok(common::converted(
        openai_models::ListModelsResponseBody {
            data,
            object: openai_models::ListObject::List,
            rest: Default::default(),
        },
        report,
    ))
}

pub fn gemini_to_claude_list(
    input: gemini_models::ListModelsResponseBody,
    supplements: &BTreeMap<String, ClaudeModelSupplement>,
    page: &ListPageFacts,
) -> Result<Converted<claude_models::ListModelsResponseBody>, TransformError> {
    validate_source_page(page, input.next_page_token.is_some(), false)?;
    let mut report = Report::default();
    let source_models = input.models.unwrap_or_default();
    let mut data = Vec::with_capacity(source_models.len());
    for model in source_models {
        let supplement = supplement_for(supplements, &model.name)?;
        let converted = get::gemini_to_claude(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        data.push(converted.value);
    }
    unique_ids(data.iter().map(|model| model.id.as_str()))?;
    let (first_id, last_id, has_more) = claude_page(&data, page)?;
    Ok(common::converted(
        claude_models::ListModelsResponseBody {
            first_id,
            last_id,
            has_more,
            data,
            rest: Default::default(),
        },
        report,
    ))
}

fn claude_page(
    models: &[claude_models::ModelInfo],
    facts: &ListPageFacts,
) -> Result<(String, String, bool), TransformError> {
    let derived_first = models.first().map(|model| model.id.clone());
    let derived_last = models.last().map(|model| model.id.clone());
    let first_id = match (&facts.first_id, derived_first) {
        (Some(supplied), Some(derived)) if supplied != &derived => {
            return Err(TransformError::new(
                crate::transform::TransformErrorKind::Conflict,
                "pagination.first_id",
                "page fact disagrees with converted list",
            ));
        }
        (Some(supplied), _) => supplied.clone(),
        (None, Some(derived)) if facts.complete => derived,
        _ => {
            return Err(missing_pagination(
                "Claude first_id requires complete list or explicit page facts (NeedsPagination)",
            ));
        }
    };
    let last_id = match (&facts.last_id, derived_last) {
        (Some(supplied), Some(derived)) if supplied != &derived => {
            return Err(TransformError::new(
                crate::transform::TransformErrorKind::Conflict,
                "pagination.last_id",
                "page fact disagrees with converted list",
            ));
        }
        (Some(supplied), _) => supplied.clone(),
        (None, Some(derived)) if facts.complete => derived,
        _ => {
            return Err(missing_pagination(
                "Claude last_id requires complete list or explicit page facts (NeedsPagination)",
            ));
        }
    };
    let has_more = match (facts.has_more, facts.complete) {
        (Some(value), _) => value,
        (None, true) => false,
        (None, false) => {
            return Err(missing_pagination(
                "Claude has_more requires explicit page facts (NeedsPagination)",
            ));
        }
    };
    Ok((first_id, last_id, has_more))
}
