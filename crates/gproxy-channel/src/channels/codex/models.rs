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
    wire::openai::models::{ListModelsResponseBody, ListObject, Model, ModelObject},
};
use http::{HeaderValue, Method, StatusCode, header};
use serde_json::json;
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
    let catalog: ListModelsResponseBody = serde_json::from_slice(&bytes)
        .map_err(|e| invalid_response(format!("Codex model catalog: {e}")))?;
    let data = catalog
        .models
        .ok_or_else(|| invalid_response("Codex model catalog is missing models"))?
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
        serde_json::to_vec(&ListModelsResponseBody {data,object:ListObject::List,models:None,rest})
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

fn normalize(mut model: Model) -> Result<Model, ChannelError> {
    if let Some(slug) = model.slug.take() {
        model.id = slug;
    }
    if model.id.is_empty() {
        return Err(invalid_response("Codex model slug missing"));
    }
    model.object = Some(ModelObject::Model);
    let base_instructions = model.base_instructions.take();
    if model.instructions.is_none() {
        model.instructions = base_instructions;
    }
    if model.thinking_supported.is_none()
        && let Some(levels) = &model.supported_reasoning_levels
    {
        model.thinking_supported = Some(!levels.is_empty());
    }
    if let Some(ceiling) = model.max_context_window.filter(|value| *value > 0) {
        model.context_window = Some(ceiling);
    }
    Ok(model)
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
