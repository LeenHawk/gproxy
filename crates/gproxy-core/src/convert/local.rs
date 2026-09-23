//! Local operations over the configured model catalog and tokenizer.
use crate::{CoreError, CoreResult, RequestContext};
use gproxy_protocol::{Dialect, HttpBody, Operation, WireRequest, WireResponse, connection::Bytes};
use http::{HeaderMap, HeaderValue, StatusCode, header::CONTENT_TYPE};
use serde_json::{Value, json};

pub(crate) fn run(
    context: &RequestContext,
    request: &WireRequest<HttpBody>,
) -> CoreResult<WireResponse<HttpBody>> {
    let key = context.operation;
    let provider = &context.target.provider;
    let mut status = StatusCode::OK;
    let body = match key.operation {
        Operation::CountTokens => {
            let HttpBody::Bytes(bytes) = &request.body else {
                return Err(CoreError::InvalidTarget(
                    "local counting requires a buffered request".into(),
                ));
            };
            if bytes.len() as u64 > context.snapshot.limits.max_request_body_bytes {
                return Err(CoreError::InvalidTarget(
                    "local request exceeds the request body limit".into(),
                ));
            }
            let count = context
                .snapshot
                .local_tokenizer
                .count_input(
                    key,
                    &provider.entity.id,
                    context.target.upstream_model.as_deref(),
                    bytes,
                )
                .map_err(CoreError::InvalidTarget)?;
            match key.dialect {
                Dialect::OpenAi => json!({"object":"response.input_tokens","input_tokens":count}),
                Dialect::Claude => json!({"input_tokens":count}),
                Dialect::Gemini => json!({"totalTokens":count}),
                _ => {
                    return Err(CoreError::InvalidTarget(
                        "no local token counter for this protocol".into(),
                    ));
                }
            }
        }
        Operation::ListModels | Operation::GetModel => {
            let mut models: Vec<Value> = provider
                .models
                .iter()
                .filter(|m| m.enabled)
                .map(|m| {
                    let mut value = m
                        .metadata
                        .get("supplements")
                        .and_then(|s| s.get(key.dialect.id()))
                        .cloned()
                        .filter(Value::is_object)
                        .unwrap_or_else(|| json!({}));
                    let object = value.as_object_mut().expect("model object");
                    match key.dialect {
                        Dialect::Gemini => {
                            object.insert(
                                "name".into(),
                                json!(format!(
                                    "models/{}",
                                    m.upstream_name.trim_start_matches("models/")
                                )),
                            );
                        }
                        Dialect::Claude => {
                            object.insert("id".into(), json!(m.upstream_name));
                            object.insert("type".into(), json!("model"));
                            object
                                .entry("display_name")
                                .or_insert_with(|| json!(m.upstream_name));
                            object
                                .entry("created_at")
                                .or_insert_with(|| json!("1970-01-01T00:00:00Z"));
                        }
                        _ => {
                            object.insert("id".into(), json!(m.upstream_name));
                            object.insert("object".into(), json!("model"));
                            object.entry("created").or_insert(json!(0));
                            object
                                .entry("owned_by")
                                .or_insert_with(|| json!(provider.entity.name));
                        }
                    }
                    value
                })
                .collect();
            models.sort_by_key(|m| {
                m.get("id")
                    .or_else(|| m.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            });
            if key.operation == Operation::GetModel {
                let name = context
                    .target
                    .upstream_model
                    .as_deref()
                    .unwrap_or_default()
                    .trim_start_matches("models/");
                models.into_iter().find(|m| m.get("id").or_else(|| m.get("name")).and_then(Value::as_str).is_some_and(|id| id.trim_start_matches("models/") == name))
                    .unwrap_or_else(|| { status = StatusCode::NOT_FOUND; json!({"error":{"type":"not_found_error","message":"model is not in the local catalog"}}) })
            } else {
                match key.dialect {
                    Dialect::Gemini => json!({"models":models}),
                    Dialect::Claude => {
                        json!({"first_id":models.first().and_then(|m|m.get("id")).cloned().unwrap_or(json!("")),"last_id":models.last().and_then(|m|m.get("id")).cloned().unwrap_or(json!("")),"has_more":false,"data":models})
                    }
                    _ => json!({"object":"list","data":models}),
                }
            }
        }
        _ => {
            return Err(CoreError::InvalidTarget(
                "this operation has no local handler".into(),
            ));
        }
    };
    let bytes = serde_json::to_vec(&body).map_err(|e| CoreError::InvalidTarget(e.to_string()))?;
    if bytes.len() as u64 > context.snapshot.limits.max_response_body_bytes {
        return Err(CoreError::InvalidTarget(
            "local response exceeds the response body limit".into(),
        ));
    }
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(Bytes::from(bytes)),
    })
}
