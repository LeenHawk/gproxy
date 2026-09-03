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
        let output = openai::ModelListResponse {
            data: list.models.into_iter().map(gemini_to_openai).collect(),
            object: openai::ListObjectType::List,
            rest: Default::default(),
        };
        return encode(&output);
    }
    encode(&gemini_to_openai(serde_json::from_slice(&body)?))
}

fn from_openai_to_gemini(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<openai::ModelListResponse>(&body) {
        let output = gemini::ListModelsResponse {
            models: list
                .data
                .into_iter()
                .map(openai_to_gemini)
                .collect::<Result<_, _>>()?,
            next_page_token: None,
            rest: Default::default(),
        };
        return encode(&output);
    }
    encode(&openai_to_gemini(serde_json::from_slice(&body)?)?)
}

fn from_gemini_to_claude(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<gemini::ListModelsResponse>(&body) {
        let has_more = list.next_page_token.is_some();
        let data = list
            .models
            .into_iter()
            .map(gemini_to_claude)
            .collect::<Vec<_>>();
        let output = claude::ListModelsResponse {
            first_id: data.first().map(|model| wire(&model.id)).transpose()?,
            last_id: list
                .next_page_token
                .or(data.last().map(|model| wire(&model.id)).transpose()?),
            data,
            has_more: Some(has_more),
            rest: Default::default(),
        };
        return encode(&output);
    }
    encode(&gemini_to_claude(serde_json::from_slice(&body)?))
}

fn from_claude_to_gemini(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<claude::ListModelsResponse>(&body) {
        let output = gemini::ListModelsResponse {
            models: list
                .data
                .into_iter()
                .map(claude_to_gemini)
                .collect::<Result<_, _>>()?,
            next_page_token: list
                .has_more
                .unwrap_or(false)
                .then_some(list.last_id)
                .flatten(),
            rest: Default::default(),
        };
        return encode(&output);
    }
    encode(&claude_to_gemini(serde_json::from_slice(&body)?)?)
}

fn gemini_to_openai(model: gemini::Model) -> openai::Model {
    openai::Model {
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
    }
}

fn openai_to_gemini(model: openai::Model) -> Result<gemini::Model, TransformError> {
    let id = wire(&model.id)?;
    Ok(gemini::Model {
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
    })
}

fn gemini_to_claude(model: gemini::Model) -> claude::ModelInfo {
    let id = gemini_id(&model);
    claude::ModelInfo {
        id: id.clone().into(),
        allowed_fallback_models: None,
        type_: claude::ModelObjectType::Known(claude::ModelObjectTypeKnown::Model),
        created_at: Some("1970-01-01T00:00:00Z".into()),
        display_name: model.display_name.or(Some(id)),
        max_input_tokens: model.input_token_limit.map(nonnegative_u64),
        max_tokens: model.output_token_limit.map(nonnegative_u64),
        capabilities: None,
        rest: Default::default(),
    }
}

fn claude_to_gemini(model: claude::ModelInfo) -> Result<gemini::Model, TransformError> {
    let id = wire(&model.id)?;
    Ok(gemini::Model {
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
    })
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
