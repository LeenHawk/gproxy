//! Codex catalogs normalized for the host's model directory. Client-specific
//! projection, permission filtering and aliases belong to the host, not here.
use super::common::{invalid_config, invalid_response};
use super::headers::{account, backend_headers, base_urls};
use super::{CLI_HEADERS, CLI_VERSION, CodexConfig};
use crate::channel::{ChannelError, HeaderAllowlist, OperationContext, PrepareContext};
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, Operation, OperationKey, WireResponse,
    connection::Bytes,
    wire::openai::models::{ListModelsResponseBody, ListObject, Model},
};
use http::{HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

const MAX_CATALOG_BYTES: usize = 16 * 1024 * 1024;

pub(super) fn prepare(ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
    let config = CodexConfig::from_view(ctx.provider)?;
    let (base, _) = base_urls(ctx.provider);
    // The captured CLI only has /models. GetModel searches this directory.
    let default = format!("{base}/models");
    let mut url = Url::parse(ctx.endpoint_override.unwrap_or(&default))
        .map_err(|e| invalid_config(e.to_string()))?;
    if let Some(query) = &ctx.request.query {
        url.query_pairs_mut()
            .extend_pairs(url::form_urlencoded::parse(query.as_bytes()));
    }
    if !url.query_pairs().any(|(key, _)| key == "client_version") {
        url.query_pairs_mut()
            .append_pair("client_version", CLI_VERSION);
    }
    let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
    let mut headers = backend_headers(
        &config,
        &account(&ctx.credential)?,
        Some((&ctx.request.headers, allowlist.as_ref())),
    )?;
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    // The normalized representation has different validators from the raw one.
    headers.remove(header::IF_NONE_MATCH);
    headers.remove(header::IF_MODIFIED_SINCE);
    let mut request = http::Request::builder()
        .method(Method::GET)
        .uri(url.as_str())
        .body(HttpBody::Bytes(Bytes::new()))
        .map_err(|e| invalid_config(e.to_string()))?;
    *request.headers_mut() = headers;
    Ok(request)
}

pub(super) async fn invoke(
    operation: Operation,
    ctx: OperationContext<'_>,
) -> Result<WireResponse, ChannelError> {
    let requested = if operation == Operation::GetModel {
        let segment = ctx
            .request
            .path
            .rsplit_once("/models/")
            .map(|(_, id)| id)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| invalid_config("model id missing from request path"))?;
        // Form decoding is reused with literal '+' escaped: model paths use
        // percent encoding, where '+' is not a space.
        let encoded = format!("id={}", segment.replace('+', "%2B"));
        Some(
            url::form_urlencoded::parse(encoded.as_bytes())
                .next()
                .unwrap()
                .1
                .into_owned(),
        )
    } else {
        None
    };
    let request = prepare(PrepareContext {
        provider: ctx.provider,
        credential: ctx.credential,
        operation: OperationKey {
            operation,
            dialect: ctx.dialect,
        },
        request: ctx.request,
        endpoint_override: ctx.endpoint_override,
    })?;
    let mut response = ctx.client.send(request).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let bytes = read(response.body).await?;
    #[derive(Deserialize)]
    struct Catalog {
        models: Vec<Value>,
        #[serde(default, flatten)]
        rest: gproxy_protocol::Rest,
    }
    let catalog: Catalog = serde_json::from_slice(&bytes)
        .map_err(|e| invalid_response(format!("Codex model catalog: {e}")))?;
    let data = catalog
        .models
        .into_iter()
        .map(normalize)
        .collect::<Result<Vec<_>, _>>()?;
    let body=if let Some(id)=requested {
        match data.into_iter().find(|model|model.id==id) {
            Some(model)=>serde_json::to_vec(&model),
            None=>{
                response.status=StatusCode::NOT_FOUND;
                serde_json::to_vec(&json!({"error":{"type":"invalid_request_error","code":"model_not_found","message":format!("Model '{id}' not found")}}))
            }
        }
    } else {
        let mut rest=catalog.rest;rest.remove("data");rest.remove("object");
        serde_json::to_vec(&ListModelsResponseBody {data,object:ListObject::List,rest})
    }.map_err(|e|invalid_response(e.to_string()))?;
    for name in [
        header::CONTENT_LENGTH,
        header::ETAG,
        header::LAST_MODIFIED,
        header::CONTENT_ENCODING,
    ] {
        response.headers.remove(name);
    }
    response.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response.body = HttpBody::Bytes(Bytes::from(body));
    Ok(response)
}

fn normalize(mut value: Value) -> Result<Model, ChannelError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| invalid_response("Codex model must be an object"))?;
    let id = object
        .get("slug")
        .or_else(|| object.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_response("Codex model slug missing"))?
        .to_owned();
    object.insert("id".into(), json!(id));
    object.insert("object".into(), json!("model"));
    // Keep original Codex fields too; the host can later project them without
    // reverse-engineering synthesized metadata or losing the original limits.
    if !object.contains_key("instructions")
        && let Some(instructions) = object.get("base_instructions").cloned()
    {
        object.insert("instructions".into(), instructions);
    }
    if !object.contains_key("thinking_supported")
        && let Some(levels) = object
            .get("supported_reasoning_levels")
            .and_then(Value::as_array)
    {
        let supported = !levels.is_empty();
        object.insert("thinking_supported".into(), json!(supported));
    }
    if let Some(ceiling) = object
        .get("max_context_window")
        .and_then(Value::as_u64)
        .filter(|v| *v > 0)
    {
        object.insert("context_window".into(), json!(ceiling));
    }
    if let Some(levels) = object
        .get_mut("supported_reasoning_levels")
        .and_then(Value::as_array_mut)
    {
        for level in levels {
            if let Some(name) = level.as_str() {
                *level = json!({"effort":name,"description":""});
            }
        }
    }
    object.remove("slug");
    object.remove("base_instructions");
    serde_json::from_value(value).map_err(|e| invalid_response(format!("Codex model: {e}")))
}

async fn read(body: HttpBody) -> Result<Bytes, ChannelError> {
    let bytes = match body {
        HttpBody::Bytes(bytes) => bytes,
        HttpBody::Stream(mut stream) => {
            let mut data = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| {
                    ChannelError::Transport(
                        gproxy_protocol::capability::CapabilityError::with_source(
                            gproxy_protocol::capability::CapabilityErrorKind::Transport,
                            gproxy_protocol::capability::CapabilityErrorStage::BodyTransfer,
                            "model catalog read failed",
                            e,
                        ),
                    )
                })?;
                if data.len().saturating_add(chunk.len()) > MAX_CATALOG_BYTES {
                    return Err(invalid_response("model catalog exceeds read limit"));
                }
                data.extend_from_slice(&chunk);
            }
            Bytes::from(data)
        }
    };
    if bytes.len() > MAX_CATALOG_BYTES {
        return Err(invalid_response("model catalog exceeds read limit"));
    }
    Ok(bytes)
}
