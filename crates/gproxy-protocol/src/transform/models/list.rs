use super::{
    common, get,
    supplement::{
        ClaudeModelSupplement, Converted, GeminiModelSupplement, ListPageFacts,
        OpenAiModelSupplement, missing_pagination,
    },
};
use crate::transform::{Report, TransformError};
use crate::wire::DeclaredFields;
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

/// Reconcile the two OpenAI directory representations using the same typed model.
/// Standard identity/timestamps and alternate catalog metadata describe one resource.
pub fn normalize_openai_list(
    input: openai_models::ListModelsResponseBody,
) -> Result<openai_models::ListModelsResponseBody, TransformError> {
    let mut input = input.into_declared();
    for model in &mut input.data {
        model.id = common::openai_identity(model)?.to_owned();
    }
    let mut index: BTreeMap<String, usize> = input
        .data
        .iter()
        .enumerate()
        .map(|(index, model)| (model.id.clone(), index))
        .collect();
    let mut seen = BTreeSet::new();
    for mut model in input.models.take().unwrap_or_default() {
        model.id = common::openai_identity(&model)?.to_owned();
        if !seen.insert(model.id.clone()) {
            return Err(TransformError::shape("models", "repeated model identity"));
        }
        if let Some(&position) = index.get(&model.id) {
            merge_openai_model(&mut input.data[position], model)?;
        } else {
            index.insert(model.id.clone(), input.data.len());
            input.data.push(model);
        }
    }
    Ok(input)
}

fn merge_openai_model(
    target: &mut openai_models::Model,
    mut source: openai_models::Model,
) -> Result<(), TransformError> {
    macro_rules! merge {
        ($($field:ident),* $(,)?) => {$(
            target.$field = common::choose_optional(
                target.$field.take(), source.$field.take(),
                &format!("model[{}].{}", target.id, stringify!($field)),
            )?;
        )*};
    }
    merge!(
        slug,
        created,
        object,
        owned_by,
        display_name,
        description,
        instructions,
        context_window,
        max_context_window,
        max_output_tokens,
        thinking_supported,
        input_modalities,
        output_modalities,
        supported_parameters,
        supported_reasoning_levels,
        default_reasoning_level,
        service_tiers,
        default_service_tier,
        generation_methods,
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
    Ok(())
}

pub fn claude_to_openai_list(
    input: claude_models::ListModelsResponseBody,
    supplements: &BTreeMap<String, OpenAiModelSupplement>,
    _page: &ListPageFacts,
) -> Result<Converted<openai_models::ListModelsResponseBody>, TransformError> {
    let mut report = Report::default();
    let mut data = Vec::with_capacity(input.data.len());
    for model in input.data {
        let supplement = supplement_for(supplements, &model.id)?;
        let converted = get::claude_to_openai(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        data.push(converted.value);
    }

    report.omitted("pagination", "OpenAI model lists have no cursor envelope");
    Ok(common::converted(
        openai_models::ListModelsResponseBody {
            data,
            object: openai_models::ListObject::List,
            models: None,
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
    let mut report = Report::default();
    let mut models = Vec::with_capacity(input.data.len());
    for model in input.data {
        let supplement = supplement_for(supplements, &model.id)?;
        let converted = get::claude_to_gemini(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        models.push(converted.value);
    }

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
    let mut report = Report::default();
    if input.models.is_some() {
        report.changed(
            "models",
            "alternate catalog reconciled with standard model data",
        );
    }
    let input = normalize_openai_list(input)?;
    let mut data = Vec::with_capacity(input.data.len());
    for model in input.data {
        let supplement = supplement_for(supplements, &model.id)?;
        let converted = get::openai_to_claude(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        data.push(converted.value);
    }

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
    let mut report = Report::default();
    if input.models.is_some() {
        report.changed(
            "models",
            "alternate catalog reconciled with standard model data",
        );
    }
    let input = normalize_openai_list(input)?;
    let mut models = Vec::with_capacity(input.data.len());
    for model in input.data {
        let supplement = supplement_for(supplements, &model.id)?;
        let converted = get::openai_to_gemini(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        models.push(converted.value);
    }

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
    _page: &ListPageFacts,
) -> Result<Converted<openai_models::ListModelsResponseBody>, TransformError> {
    let mut report = Report::default();
    let source_models = input.models.unwrap_or_default();
    let mut data = Vec::with_capacity(source_models.len());
    for model in source_models {
        let supplement = supplement_for(supplements, &model.name)?;
        let converted = get::gemini_to_openai(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        data.push(converted.value);
    }

    report.omitted("pagination", "OpenAI model lists have no cursor envelope");
    Ok(common::converted(
        openai_models::ListModelsResponseBody {
            data,
            object: openai_models::ListObject::List,
            models: None,
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
    let mut report = Report::default();
    let source_models = input.models.unwrap_or_default();
    let mut data = Vec::with_capacity(source_models.len());
    for model in source_models {
        let supplement = supplement_for(supplements, &model.name)?;
        let converted = get::gemini_to_claude(model, supplement)?;
        common::merge_report(&mut report, converted.report);
        data.push(converted.value);
    }

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
