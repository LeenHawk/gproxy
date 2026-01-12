use crate::formats::{gemini, openai};

use super::{
    ensure_empty_query, ensure_path_eq, extract_path_suffix, gemini_model_from_openai,
    map_status_headers, normalize_gemini_name, RequestParts, ResponseParts, TransformError,
    GEMINI_MODELS_PATH, OPENAI_MODELS_PATH,
};

pub fn list_request(req: RequestParts<()>) -> Result<RequestParts<()>, TransformError> {
    ensure_path_eq(&req.path, OPENAI_MODELS_PATH)?;
    ensure_empty_query(&req.query)?;
    Ok(RequestParts {
        path: GEMINI_MODELS_PATH.to_string(),
        query: req.query,
        headers: req.headers,
        body: (),
    })
}

pub fn get_request(req: RequestParts<()>) -> Result<RequestParts<()>, TransformError> {
    ensure_empty_query(&req.query)?;
    let model_id = extract_path_suffix(&req.path, OPENAI_MODELS_PATH)?;
    let name = normalize_gemini_name(&model_id);
    Ok(RequestParts {
        path: format!("{GEMINI_MODELS_PATH}/{name}"),
        query: req.query,
        headers: req.headers,
        body: (),
    })
}

pub fn list_response(
    resp: ResponseParts<openai::models_list::ModelsListResponse>,
) -> Result<ResponseParts<gemini::models_list::ModelsListResponse>, TransformError> {
    let models = resp
        .body
        .data
        .iter()
        .map(gemini_model_from_openai)
        .collect();
    let body = gemini::models_list::ModelsListResponse {
        models,
        next_page_token: None,
    };
    map_status_headers(resp.status, &resp.headers, body)
}

pub fn get_response(
    resp: ResponseParts<openai::model_get::ModelGetResponse>,
) -> Result<ResponseParts<gemini::model_get::ModelGetResponse>, TransformError> {
    let body = gemini_model_from_openai(&resp.body);
    map_status_headers(resp.status, &resp.headers, body)
}
