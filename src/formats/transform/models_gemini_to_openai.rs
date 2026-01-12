use crate::formats::{gemini, openai};

use super::{
    ensure_empty_query, extract_gemini_name, map_status_headers, openai_model_from_gemini,
    openai_id_from_gemini, RequestParts, ResponseParts, TransformError, GEMINI_MODELS_PATH,
    GEMINI_MODELS_PATH_BETA, OPENAI_MODELS_PATH,
};

pub fn list_request(req: RequestParts<()>) -> Result<RequestParts<()>, TransformError> {
    if req.path.trim_end_matches('/') != GEMINI_MODELS_PATH
        && req.path.trim_end_matches('/') != GEMINI_MODELS_PATH_BETA
    {
        return Err(TransformError::Invalid("models list path"));
    }
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
    let name = extract_gemini_name(&req.path)?;
    let model_id = openai_id_from_gemini(&name);
    Ok(RequestParts {
        path: format!("{OPENAI_MODELS_PATH}/{model_id}"),
        query: req.query,
        headers: req.headers,
        body: (),
    })
}

pub fn list_response(
    resp: ResponseParts<gemini::models_list::ModelsListResponse>,
) -> Result<ResponseParts<openai::models_list::ModelsListResponse>, TransformError> {
    let data = resp
        .body
        .models
        .iter()
        .map(openai_model_from_gemini)
        .collect();
    let body = openai::models_list::ModelsListResponse {
        object_type: openai::models_list::ListObjectType::List,
        data,
    };
    map_status_headers(resp.status, &resp.headers, body)
}

pub fn get_response(
    resp: ResponseParts<gemini::model_get::ModelGetResponse>,
) -> Result<ResponseParts<openai::model_get::ModelGetResponse>, TransformError> {
    let body = openai_model_from_gemini(&resp.body);
    map_status_headers(resp.status, &resp.headers, body)
}
