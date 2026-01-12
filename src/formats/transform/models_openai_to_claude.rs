use crate::formats::{claude, openai};

use super::{
    claude_model_from_openai, ensure_empty_query, ensure_path_eq, extract_path_suffix,
    map_status_headers, RequestParts, ResponseParts, TransformError, CLAUDE_MODELS_PATH,
    OPENAI_MODELS_PATH,
};

pub fn list_request(req: RequestParts<()>) -> Result<RequestParts<()>, TransformError> {
    ensure_path_eq(&req.path, OPENAI_MODELS_PATH)?;
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
    let model_id = extract_path_suffix(&req.path, OPENAI_MODELS_PATH)?;
    Ok(RequestParts {
        path: format!("{CLAUDE_MODELS_PATH}/{model_id}"),
        query: req.query,
        headers: req.headers,
        body: (),
    })
}

pub fn list_response(
    resp: ResponseParts<openai::models_list::ModelsListResponse>,
) -> Result<ResponseParts<claude::models_list::ModelsListResponse>, TransformError> {
    let data = resp
        .body
        .data
        .iter()
        .map(claude_model_from_openai)
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
    resp: ResponseParts<openai::model_get::ModelGetResponse>,
) -> Result<ResponseParts<claude::model_get::ModelGetResponse>, TransformError> {
    let body = claude_model_from_openai(&resp.body);
    map_status_headers(resp.status, &resp.headers, body)
}
