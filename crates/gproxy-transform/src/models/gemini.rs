use bytes::Bytes;
use gproxy_protocol::{WireFamily, claude, gemini, openai};

use crate::TransformError;

pub(crate) fn response(
    source: WireFamily,
    target: WireFamily,
    body: Bytes,
) -> Result<Bytes, TransformError> {
    match (source, target) {
        (WireFamily::OpenAi, WireFamily::Gemini) => from_gemini_to_openai(body),
        (WireFamily::Gemini, WireFamily::OpenAi) => from_openai_to_gemini(body),
        (WireFamily::Claude, WireFamily::Gemini) => from_gemini_to_claude(body),
        (WireFamily::Gemini, WireFamily::Claude) => from_claude_to_gemini(body),
        _ => Err(TransformError::shape(
            "models",
            "unsupported Gemini model pair",
        )),
    }
}

fn from_gemini_to_openai(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<gemini::ListModelsResponse>(&body) {
        let output = crate::typed::models::openai_to_gemini::list_response(list);
        return encode(&output);
    }
    encode(&crate::typed::models::openai_to_gemini::get_response(
        serde_json::from_slice(&body)?,
    ))
}

fn from_openai_to_gemini(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<openai::ModelListResponse>(&body) {
        let output = crate::typed::models::gemini_to_openai::list_response(list)?;
        return encode(&output);
    }
    encode(&crate::typed::models::gemini_to_openai::get_response(
        serde_json::from_slice(&body)?,
    )?)
}

fn from_gemini_to_claude(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<gemini::ListModelsResponse>(&body) {
        let output = crate::typed::models::claude_to_gemini::list_response(list)?;
        return encode(&output);
    }
    encode(&crate::typed::models::claude_to_gemini::get_response(
        serde_json::from_slice(&body)?,
    ))
}

fn from_claude_to_gemini(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<claude::ListModelsResponse>(&body) {
        let output = crate::typed::models::gemini_to_claude::list_response(list)?;
        return encode(&output);
    }
    encode(&crate::typed::models::gemini_to_claude::get_response(
        serde_json::from_slice(&body)?,
    )?)
}

pub(crate) fn gemini_to_openai(model: gemini::Model) -> openai::Model {
    crate::wire!(openai::Model {
        id: gemini_id(&model).into(),
        created: None,
        display_name: model.display_name,
        context_window: model.input_token_limit.map(nonnegative_u64),
        max_context_window: None,
        max_output_tokens: model.output_token_limit.map(nonnegative_u64),
        thinking_supported: model.thinking,
        object: openai::ModelObjectType::Model,
        owned_by: Some("google".into()),
        rest: Default::default(),
    })
}

pub(crate) fn openai_to_gemini(model: openai::Model) -> Result<gemini::Model, TransformError> {
    let id = wire(&model.id)?;
    Ok(crate::wire!(gemini::Model {
        name: Some(format!("models/{id}")),
        base_model_id: Some(id.clone()),
        version: None,
        display_name: model.display_name.or(Some(id)),
        description: None,
        input_token_limit: model.context_window.map(saturating_i32),
        output_token_limit: model.max_output_tokens.map(saturating_i32),
        supported_generation_methods: Vec::new(),
        supported_actions: Vec::new(),
        thinking: model.thinking_supported,
        temperature: None,
        max_temperature: None,
        top_p: None,
        top_k: None,
        rest: Default::default(),
    }))
}

pub(crate) fn gemini_to_claude(model: gemini::Model) -> claude::ModelInfo {
    let id = gemini_id(&model);
    crate::wire!(claude::ModelInfo {
        id: id.clone().into(),
        allowed_fallback_models: None,
        type_: claude::ModelObjectType::Known(claude::ModelObjectTypeKnown::Model),
        created_at: Some("1970-01-01T00:00:00Z".into()),
        display_name: model.display_name.or(Some(id)),
        max_input_tokens: model.input_token_limit.map(nonnegative_u64),
        max_tokens: model.output_token_limit.map(nonnegative_u64),
        capabilities: None,
        rest: Default::default(),
    })
}

pub(crate) fn claude_to_gemini(model: claude::ModelInfo) -> Result<gemini::Model, TransformError> {
    let id = wire(&model.id)?;
    Ok(crate::wire!(gemini::Model {
        name: Some(format!("models/{id}")),
        base_model_id: Some(id.clone()),
        version: None,
        display_name: model.display_name.or(Some(id)),
        description: None,
        input_token_limit: model.max_input_tokens.map(saturating_i32),
        output_token_limit: model.max_tokens.map(saturating_i32),
        supported_generation_methods: Vec::new(),
        supported_actions: Vec::new(),
        thinking: model
            .capabilities
            .map(|capabilities| capabilities.thinking.supported),
        temperature: None,
        max_temperature: None,
        top_p: None,
        top_k: None,
        rest: Default::default(),
    }))
}

fn gemini_id(model: &gemini::Model) -> String {
    model
        .base_model_id
        .clone()
        .or_else(|| {
            model
                .name
                .as_deref()
                .and_then(|name| name.strip_prefix("models/"))
                .map(str::to_owned)
        })
        .or_else(|| model.name.clone())
        .unwrap_or_else(|| "unknown".into())
}

fn wire<T: serde::Serialize>(value: &T) -> Result<String, TransformError> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| TransformError::shape("model id", "expected string"))
}

fn nonnegative_u64(value: i32) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

fn saturating_i32(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn encode(value: &impl serde::Serialize) -> Result<Bytes, TransformError> {
    Ok(Bytes::from(serde_json::to_vec(value)?))
}
