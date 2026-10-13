//! Publisher catalogues are not Gemini model lists. Fetch both publishers,
//! drain their pages, and normalize names for the host's existing discovery.

use std::collections::{BTreeMap, BTreeSet};

use futures_util::StreamExt;
use gproxy_protocol::{HttpBody, Operation, OperationKey, WireResponse, connection::Bytes};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

use super::Vertex;
use crate::channel::{BaseChannel, ChannelError, OperationContext, PrepareContext};

const MAX_MODEL_BODY: usize = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    // An empty protobuf repeated field may be omitted. An explicit endpoint
    // override can also return an already normalized Gemini directory.
    #[serde(default, alias = "models")]
    publisher_models: Vec<Value>,
    #[serde(default)]
    next_page_token: String,
}

pub(super) async fn list(
    channel: &Vertex,
    context: OperationContext<'_>,
) -> Result<WireResponse, ChannelError> {
    let request = channel.prepare(PrepareContext {
        provider: context.provider,
        credential: context.credential,
        operation: OperationKey {
            operation: Operation::ListModels,
            dialect: context.dialect,
        },
        request: context.request,
        endpoint_override: context.endpoint_override,
    })?;
    let (parts, _) = request.into_parts();
    let base = Url::parse(&parts.uri.to_string())
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
    // A configured complete endpoint is authoritative: never manufacture an
    // Anthropic URL from it or fetch the same override twice.
    let publishers: &[&str] = if context
        .endpoint_override
        .is_some_and(|url| !url.trim().is_empty())
    {
        &["google"]
    } else {
        &["google", "anthropic"]
    };
    let mut models = BTreeMap::new();
    for publisher in publishers {
        let mut url = base.clone();
        if *publisher == "anthropic" {
            let path = url
                .path()
                .replace("/publishers/google/", "/publishers/anthropic/");
            url.set_path(&path);
        }
        let mut tokens = BTreeSet::new();
        loop {
            let mut request = http::Request::builder()
                .method(Method::GET)
                .uri(url.as_str())
                .body(HttpBody::Bytes(Bytes::new()))
                .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
            *request.headers_mut() = parts.headers.clone();
            request.headers_mut().remove(header::CONTENT_LENGTH);
            let response = context.client.send(request).await?;
            // Do not report a partial directory as a successful refresh.
            if !response.status.is_success() {
                return Ok(response);
            }
            let page: Page = serde_json::from_slice(&read_body(response.body).await?)
                .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
            for model in page.publisher_models {
                let model = normalize(model)?;
                models
                    .entry(model["name"].as_str().unwrap().to_owned())
                    .or_insert(model);
            }
            if page.next_page_token.is_empty() {
                break;
            }
            if !tokens.insert(page.next_page_token.clone()) {
                return Err(ChannelError::InvalidResponse(
                    "Vertex model directory repeated a page token".into(),
                ));
            }
            let query: Vec<_> = url
                .query_pairs()
                .filter(|(key, _)| key != "pageToken")
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect();
            url.query_pairs_mut()
                .clear()
                .extend_pairs(query)
                .append_pair("pageToken", &page.next_page_token);
        }
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Bytes(Bytes::from(
            serde_json::to_vec(&json!({"models": models.into_values().collect::<Vec<_>>()}))
                .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?,
        )),
    })
}

/// A PublisherModel has the same identity rules whether fetched alone or in
/// a directory. Return the Gemini shape declared by this channel in both.
pub(super) async fn get(
    channel: &Vertex,
    context: OperationContext<'_>,
) -> Result<WireResponse, ChannelError> {
    let request = channel.prepare(PrepareContext {
        provider: context.provider,
        credential: context.credential,
        operation: OperationKey {
            operation: Operation::GetModel,
            dialect: context.dialect,
        },
        request: context.request,
        endpoint_override: context.endpoint_override,
    })?;
    let mut response = context.client.send(request).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let value = serde_json::from_slice(&read_body(response.body).await?)
        .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
    response.body = HttpBody::Bytes(Bytes::from(
        serde_json::to_vec(&normalize(value)?)
            .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?,
    ));
    response.headers.remove(header::CONTENT_LENGTH);
    response.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

fn normalize(mut model: Value) -> Result<Value, ChannelError> {
    let name = model.get("name").and_then(Value::as_str).ok_or_else(|| {
        ChannelError::InvalidResponse("Vertex publisher model has no name".into())
    })?;
    // The publisher is part of the upstream model identity, not the gateway's
    // provider-routing prefix. Preserve the entire model remainder as well.
    let id = match name
        .strip_prefix("publishers/")
        .and_then(|resource| resource.split_once("/models/"))
    {
        Some((publisher, model)) => format!("{publisher}/{model}"),
        None => name.strip_prefix("models/").unwrap_or(name).to_owned(),
    };
    if id.is_empty() {
        return Err(ChannelError::InvalidResponse(
            "Vertex publisher model has an empty name".into(),
        ));
    }
    let fields = model.as_object_mut().expect("a named model is an object");
    fields.insert("name".into(), json!(format!("models/{id}")));
    fields.entry("baseModelId").or_insert(json!(id));
    if let Some(version) = fields.get("versionId").cloned() {
        fields.entry("version").or_insert(version);
    }
    Ok(model)
}

async fn read_body(body: HttpBody) -> Result<Bytes, ChannelError> {
    let bytes = match body {
        HttpBody::Bytes(bytes) => bytes,
        HttpBody::Stream(mut stream) => {
            let mut bytes = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk =
                    chunk.map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
                bytes.extend_from_slice(&chunk);
                check_size(bytes.len())?;
            }
            Bytes::from(bytes)
        }
    };
    check_size(bytes.len())?;
    Ok(bytes)
}

fn check_size(size: usize) -> Result<(), ChannelError> {
    if size > MAX_MODEL_BODY {
        return Err(ChannelError::InvalidResponse(
            "the Vertex model directory exceeds the read limit".into(),
        ));
    }
    Ok(())
}
