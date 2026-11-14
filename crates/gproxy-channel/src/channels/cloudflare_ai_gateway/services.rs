//! Model listing and reranking, neither of which Cloudflare serves in an
//! OpenAI shape.
//!
//! The model directory is `GET /ai/models/search`, a paged Cloudflare
//! envelope `{success, result: [{name, description, task: {name},
//! properties: [{property_id, value}]}], result_info}`. It lists the Workers AI
//! (`@cf/`) catalogue only; third-party models routed by the gateway have no
//! directory and are added by hand. Only the tasks this channel can serve are
//! kept, and the rows become an OpenAI model list.
//!
//! Reranking is a Workers AI model run, `POST /ai/run/{model}` with
//! `{query, contexts: [{text}], top_k}`, answering
//! `{result: {response: [{id, score}], meta: {cost_metric_name_1:
//! "input_tokens", cost_metric_value_1}}}`. The OpenAI-dialect rerank request
//! is rewritten into that body and the reply back into `{model, results:
//! [{index, relevance_score, document}], usage}`; Cloudflare does not echo the
//! documents, so they are taken from the request.

use super::CloudflareAiGateway;
use crate::channel::{
    BaseChannel, ChannelError, CredentialView, OperationContext, PrepareContext, ProviderView,
};
use crate::channels::shared::compatible::http::{invalid_response, read_body};
use gproxy_client::OutboundClient;
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse};
use http::{Method, header};
use serde_json::{Map, Value, json};
use std::sync::Arc;

/// The directory pages at most 100 rows whatever `per_page` asks for.
const PAGE_SIZE: usize = 100;
/// A bound on paging in case a reply never runs short.
const MAX_PAGES: usize = 20;
/// Directory tasks this channel serves: chat, embeddings and the reranker
/// (which Cloudflare files under text classification).
const SERVED_TASKS: [&str; 3] = ["Text Generation", "Text Embeddings", "Text Classification"];

fn invalid(message: &str) -> ChannelError {
    ChannelError::InvalidConfig(message.into())
}

impl CloudflareAiGateway {
    async fn send(
        &self,
        ctx: &Target<'_>,
        operation: Operation,
        request: WireRequest,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let prepared = self.prepare(PrepareContext {
            provider: ctx.provider,
            credential: ctx.credential,
            operation: OperationKey {
                operation,
                dialect: Dialect::OpenAi,
            },
            request,
            endpoint_override: ctx.endpoint_override,
        })?;
        Ok(ctx.client.send(prepared).await?)
    }

    pub(super) async fn models(
        &self,
        ctx: OperationContext<'_>,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let headers = ctx.request.headers;
        let ctx = Target::of(
            ctx.provider,
            ctx.credential,
            ctx.endpoint_override,
            ctx.client,
        );
        let mut rows = Vec::new();
        let mut last = None;
        for page in 1..=MAX_PAGES {
            let request = WireRequest {
                method: Method::GET,
                path: "/models/search".into(),
                query: Some(format!("per_page={PAGE_SIZE}&page={page}")),
                headers: headers.clone(),
                body: HttpBody::Bytes(Bytes::new()),
            };
            let response = self.send(&ctx, Operation::ListModels, request).await?;
            let body = read_body(response.body).await?;
            if !response.status.is_success() {
                return Ok(WireResponse {
                    status: response.status,
                    headers: response.headers,
                    body: HttpBody::Bytes(body),
                });
            }
            let payload: Value = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(error.to_string()))?;
            let page_rows = payload
                .get("result")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid_response("Cloudflare model search has no result"))?;
            let full = page_rows.len() >= PAGE_SIZE;
            rows.extend(page_rows.iter().filter_map(model));
            last = Some((response.status, response.headers));
            if !full {
                break;
            }
        }
        let (status, mut headers) = last.expect("at least one page is fetched");
        headers.remove(header::CONTENT_LENGTH);
        let body = serde_json::to_vec(&json!({"object": "list", "data": rows}))
            .map_err(|error| invalid_response(error.to_string()))?;
        Ok(WireResponse {
            status,
            headers,
            body: HttpBody::Bytes(Bytes::from(body)),
        })
    }

    pub(super) async fn rerank_run(
        &self,
        ctx: OperationContext<'_>,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let WireRequest {
            query,
            headers,
            body: source,
            ..
        } = ctx.request;
        let ctx = Target::of(
            ctx.provider,
            ctx.credential,
            ctx.endpoint_override,
            ctx.client,
        );
        let source = read_body(source)
            .await
            .map_err(|_| invalid("rerank body unreadable"))?;
        let source: Value =
            serde_json::from_slice(&source).map_err(|_| invalid("rerank body is not JSON"))?;
        let (model, documents, body) = rerank_request(&source)?;
        let request = WireRequest {
            method: Method::POST,
            path: format!("/run/{model}"),
            query,
            headers,
            body: HttpBody::Bytes(Bytes::from(body)),
        };
        let response = self.send(&ctx, Operation::Rerank, request).await?;
        let WireResponse {
            status,
            mut headers,
            body,
        } = response;
        let body = read_body(body).await?;
        if !status.is_success() {
            return Ok(WireResponse {
                status,
                headers,
                body: HttpBody::Bytes(body),
            });
        }
        let payload: Value =
            serde_json::from_slice(&body).map_err(|error| invalid_response(error.to_string()))?;
        let reply = rerank_response(&payload, &model, &documents)?;
        headers.remove(header::CONTENT_LENGTH);
        Ok(WireResponse {
            status,
            headers,
            body: HttpBody::Bytes(Bytes::from(reply)),
        })
    }
}

/// What a call needs from its context once the request body is taken: the
/// body stream is not `Sync`, so the context itself cannot be held across
/// an await by reference.
struct Target<'a> {
    provider: ProviderView<'a>,
    credential: CredentialView<'a>,
    endpoint_override: Option<&'a str>,
    client: Arc<dyn OutboundClient>,
}

impl<'a> Target<'a> {
    fn of(
        provider: ProviderView<'a>,
        credential: CredentialView<'a>,
        endpoint_override: Option<&'a str>,
        client: Arc<dyn OutboundClient>,
    ) -> Self {
        Self {
            provider,
            credential,
            endpoint_override,
            client,
        }
    }
}

fn property<'a>(row: &'a Value, id: &str) -> Option<&'a Value> {
    row.get("properties")?
        .as_array()?
        .iter()
        .find(|property| property.get("property_id").and_then(Value::as_str) == Some(id))?
        .get("value")
}

/// One directory row as an OpenAI model, or `None` for a task not served.
fn model(row: &Value) -> Option<Value> {
    let task = row.pointer("/task/name").and_then(Value::as_str)?;
    if !SERVED_TASKS.contains(&task) {
        return None;
    }
    let id = row.get("name").and_then(Value::as_str)?;
    let mut model = Map::new();
    model.insert("id".into(), id.into());
    model.insert("object".into(), "model".into());
    model.insert("owned_by".into(), "cloudflare".into());
    if let Some(description) = row.get("description").filter(|value| value.is_string()) {
        model.insert("description".into(), description.clone());
    }
    if let Some(window) = property(row, "context_window").and_then(|value| match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }) {
        model.insert("context_length".into(), window.into());
    }
    Some(Value::Object(model))
}

/// A model id goes into the `/ai/run/` path. Cloudflare's ids carry slashes
/// and `@`; anything that would leave the path or climb out of it is refused.
fn run_model(model: &str) -> Result<&str, ChannelError> {
    let model = model.trim();
    if model.is_empty()
        || model
            .split('/')
            .any(|segment| matches!(segment, "" | "." | ".."))
        || !model.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'/' | b'-' | b'_' | b'.' | b':')
        })
    {
        return Err(invalid("rerank model is not a Cloudflare model id"));
    }
    Ok(model)
}

/// The OpenAI-dialect rerank request as `(model, documents, run body)`.
fn rerank_request(source: &Value) -> Result<(String, Vec<String>, Vec<u8>), ChannelError> {
    let model = source
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("rerank request has no model"))?;
    let model = run_model(model)?.to_owned();
    let query = source
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("rerank request has no query"))?;
    let documents = source
        .get("documents")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("rerank request has no documents"))?
        .iter()
        .map(|document| match document {
            Value::String(text) => Some(text.clone()),
            document => document
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid("Cloudflare reranks text documents only"))?;
    let mut body = Map::new();
    body.insert("query".into(), query.into());
    body.insert(
        "contexts".into(),
        documents.iter().map(|text| json!({"text": text})).collect(),
    );
    if let Some(top) = source.get("top_n").filter(|value| !value.is_null()) {
        body.insert("top_k".into(), top.clone());
    }
    let body = serde_json::to_vec(&body).map_err(|error| invalid_response(error.to_string()))?;
    Ok((model, documents, body))
}

/// The run reply as an OpenAI-dialect rerank reply.
fn rerank_response(
    payload: &Value,
    model: &str,
    documents: &[String],
) -> Result<Vec<u8>, ChannelError> {
    let scores = payload
        .pointer("/result/response")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response("Cloudflare rerank reply has no response"))?;
    let results = scores
        .iter()
        .map(|score| {
            let index = score.get("id").and_then(Value::as_u64)?;
            let relevance = score.get("score").filter(|value| value.is_number())?;
            let text = documents.get(usize::try_from(index).ok()?)?;
            Some(json!({
                "index": index,
                "relevance_score": relevance,
                "document": {"text": text},
            }))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid_response("Cloudflare rerank reply has a malformed score"))?;
    let mut reply = Map::new();
    reply.insert("model".into(), model.into());
    reply.insert("results".into(), results.into());
    let meta = payload.pointer("/result/meta");
    if let Some(tokens) = (1..=3).find_map(|slot| {
        let meta = meta?;
        (meta.get(format!("cost_metric_name_{slot}"))?.as_str()? == "input_tokens")
            .then(|| meta.get(format!("cost_metric_value_{slot}"))?.as_u64())
            .flatten()
    }) {
        reply.insert("usage".into(), json!({"total_tokens": tokens}));
    }
    serde_json::to_vec(&reply).map_err(|error| invalid_response(error.to_string()))
}
