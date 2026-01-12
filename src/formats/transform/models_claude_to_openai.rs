use crate::formats::{claude, openai};

use super::{
    ensure_empty_query, ensure_path_eq, extract_path_suffix, map_status_headers,
    openai_model_from_claude, RequestParts, ResponseParts, TransformError, CLAUDE_MODELS_PATH,
    OPENAI_MODELS_PATH,
};

pub fn list_request(req: RequestParts<()>) -> Result<RequestParts<()>, TransformError> {
    ensure_path_eq(&req.path, CLAUDE_MODELS_PATH)?;
    ensure_empty_query(&req.query)?;
    Ok(RequestParts {
        path: OPENAI_MODELS_PATH.to_string(),
        query: req.query,
        headers: req.headers,
        body: (),
    })
}

pub fn get_request(req: RequestParts<()>) -> Result<RequestParts<()>, TransformError> {
    ensure_empty_query(&req.query)?;
    let model_id = extract_path_suffix(&req.path, CLAUDE_MODELS_PATH)?;
    Ok(RequestParts {
        path: format!("{OPENAI_MODELS_PATH}/{model_id}"),
        query: req.query,
        headers: req.headers,
        body: (),
    })
}

pub fn list_response(
    resp: ResponseParts<claude::models_list::ModelsListResponse>,
) -> Result<ResponseParts<openai::models_list::ModelsListResponse>, TransformError> {
    let data = resp
        .body
        .data
        .iter()
        .map(openai_model_from_claude)
        .collect();
    let body = openai::models_list::ModelsListResponse {
        object_type: openai::models_list::ListObjectType::List,
        data,
    };
    map_status_headers(resp.status, &resp.headers, body)
}

pub fn get_response(
    resp: ResponseParts<claude::model_get::ModelGetResponse>,
) -> Result<ResponseParts<openai::model_get::ModelGetResponse>, TransformError> {
    let body = openai_model_from_claude(&resp.body);
    map_status_headers(resp.status, &resp.headers, body)
}
