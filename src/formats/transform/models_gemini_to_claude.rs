use crate::formats::{claude, gemini};

use super::{
    claude_model_from_gemini, ensure_empty_query, extract_gemini_name, map_status_headers,
    openai_id_from_gemini, RequestParts, ResponseParts, TransformError, CLAUDE_MODELS_PATH,
    GEMINI_MODELS_PATH, GEMINI_MODELS_PATH_BETA,
};

pub fn list_request(req: RequestParts<()>) -> Result<RequestParts<()>, TransformError> {
    if req.path.trim_end_matches('/') != GEMINI_MODELS_PATH
        && req.path.trim_end_matches('/') != GEMINI_MODELS_PATH_BETA
    {
        return Err(TransformError::Invalid("models list path"));
    }
    ensure_empty_query(&req.query)?;
    Ok(RequestParts {
        path: CLAUDE_MODELS_PATH.to_string(),
        query: req.query,
        headers: req.headers,
        body: (),
    })
}

pub fn get_request(req: RequestParts<()>) -> Result<RequestParts<()>, TransformError> {
    ensure_empty_query(&req.query)?;
    let name = extract_gemini_name(&req.path)?;
    let model_id = openai_id_from_gemini(&name);
    Ok(RequestParts {
        path: format!("{CLAUDE_MODELS_PATH}/{model_id}"),
        query: req.query,
        headers: req.headers,
        body: (),
    })
}

pub fn list_response(
    resp: ResponseParts<gemini::models_list::ModelsListResponse>,
) -> Result<ResponseParts<claude::models_list::ModelsListResponse>, TransformError> {
    let data = resp
        .body
        .models
        .iter()
        .map(claude_model_from_gemini)
        .collect::<Vec<_>>();
    let first_id = data.first().map(|item| item.id.clone()).unwrap_or_default();
    let last_id = data.last().map(|item| item.id.clone()).unwrap_or_default();
    let body = claude::models_list::ModelsListResponse {
        data,
        first_id,
        has_more: false,
        last_id,
    };
    map_status_headers(resp.status, &resp.headers, body)
}

pub fn get_response(
    resp: ResponseParts<gemini::model_get::ModelGetResponse>,
) -> Result<ResponseParts<claude::model_get::ModelGetResponse>, TransformError> {
    let body = claude_model_from_gemini(&resp.body);
    map_status_headers(resp.status, &resp.headers, body)
}
