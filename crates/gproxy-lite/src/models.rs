//! The model list of every provider at once, each id under its provider's
//! name: the form the model field takes when more than one is configured.

use std::sync::Arc;

use gproxy_protocol::{
    Dialect, HttpBody, OperationKey, WireRequest, WireResponse, codec::read_http_body,
};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use crate::engine::{Addressed, CODEC, Inner, Refusal, json_response};

/// A provider that cannot answer (not signed in, upstream refused) is left
/// out; the others still list.
pub(crate) async fn every_provider(
    inner: &Arc<Inner>,
    operation: OperationKey,
    headers: &HeaderMap,
) -> Result<WireResponse<HttpBody>, Refusal> {
    let gemini = operation.dialect == Dialect::Gemini;
    let (path, list) = if gemini {
        ("/v1beta/models", "models")
    } else {
        ("/v1/models", "data")
    };
    let mut models = Vec::new();
    for (provider, configured) in inner.providers.iter().enumerate() {
        let wire = WireRequest {
            method: Method::GET,
            path: path.to_owned(),
            query: None,
            headers: headers.clone(),
            body: HttpBody::Bytes(Default::default()),
        };
        let addressed = Addressed {
            provider,
            model: None,
        };
        let Ok(response) = crate::exec::run(inner, addressed, operation, wire).await else {
            continue;
        };
        if !response.status.is_success() {
            continue;
        }
        let Ok(bytes) = read_http_body(response.body, CODEC).await else {
            continue;
        };
        let Ok(mut listed) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        let Some(entries) = listed.get_mut(list).and_then(Value::as_array_mut) else {
            continue;
        };
        for mut model in std::mem::take(entries) {
            let field = if gemini { "name" } else { "id" };
            let Some(id) = model.get(field).and_then(Value::as_str) else {
                continue;
            };
            let prefixed = match id.strip_prefix("models/") {
                Some(bare) if gemini => format!("models/{}/{bare}", configured.name),
                _ => format!("{}/{id}", configured.name),
            };
            model[field] = prefixed.into();
            models.push(model);
        }
    }
    let body = match operation.dialect {
        Dialect::Gemini => json!({ "models": models }),
        Dialect::Claude => json!({
            "first_id": models.first().and_then(|model| model.get("id")),
            "last_id": models.last().and_then(|model| model.get("id")),
            "has_more": false,
            "data": models,
        }),
        _ => json!({ "object": "list", "data": models }),
    };
    Ok(json_response(StatusCode::OK, &body))
}
