//! Stateless direct Interactions generation adapters. Native passthrough retains
//! incremental streaming. Cross-protocol streams are collected with bounded
//! native collectors, then emitted with the client's complete native lifecycle.
use crate::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    adapt::generate::{
        Endpoint,
        stream::{
            event::{EventLimits, NativeEvent},
            reader::{NativeFrame, NativeReader, SourceFraming},
            synthesize::CompleteResponse,
        },
    },
    capability::Upstream,
    codec::{CodecLimits, encode_json, encode_sse_done, encode_sse_event, read_http_body},
    connection::Bytes,
    transform::{
        Converted, Report, TransformError, TransformErrorKind,
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
        interactions::{
            self,
            stream::{InteractionStreamCollector, synthesize_interaction_stream},
        },
    },
    wire::{
        claude::{generate_content as c, stream as cs},
        gemini::interactions as i,
        openai::{chat as h, responses as r},
    },
};
use serde_json::Value;

pub struct InteractionFacts {
    pub id: String,
    pub model: String,
    pub created_at: i64,
    pub max_tokens: u64,
    pub namespace: IdNamespace,
}

pub struct InteractionBinding {
    pub client: OperationKey,
    pub native: OperationKey,
    pub endpoint: Endpoint,
    pub facts: InteractionFacts,
}

pub enum InteractionOutcome {
    Success {
        response: WireResponse<HttpBody>,
        report: Report,
    },
    Rejected(WireResponse<HttpBody>),
}

fn codec(error: crate::codec::CodecError) -> TransformError {
    let kind = match error.kind() {
        crate::codec::CodecErrorKind::Limit => TransformErrorKind::Limit,
        crate::codec::CodecErrorKind::Transport => TransformErrorKind::Host,
        _ => TransformErrorKind::InvalidResult,
    };
    TransformError::with_source(kind, "interaction.codec", error.to_string(), error)
}

pub async fn invoke<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<Value>,
    binding: InteractionBinding,
    limits: CodecLimits,
) -> Result<InteractionOutcome, TransformError> {
    let InteractionBinding {
        client,
        native,
        endpoint,
        facts,
    } = binding;
    let Converted {
        mut value,
        mut report,
    } = interactions::request(client, native, request.body, &facts.model, facts.max_tokens)?;
    value["stream"] = Value::Bool(native.operation == Operation::StreamGenerateContent);
    let mut headers = request.headers;
    for name in [
        http::header::CONTENT_LENGTH,
        http::header::CONTENT_ENCODING,
        http::header::TRANSFER_ENCODING,
    ] {
        headers.remove(name);
    }
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    let response = upstream
        .send(
            target,
            WireRequest {
                method: http::Method::POST,
                path: endpoint.path,
                query: endpoint.query,
                headers,
                body: HttpBody::Bytes(encode_json(&value, limits).map_err(codec)?),
            },
        )
        .await?;
    if !response.status.is_success() {
        return Ok(InteractionOutcome::Rejected(response));
    }
    let WireResponse {
        status,
        mut headers,
        body,
    } = response;
    let cap = usize::try_from(limits.max_body_bytes).unwrap_or(usize::MAX);
    let events = EventLimits {
        max_bytes: cap,
        max_pending_bytes: usize::try_from(limits.max_value_bytes).unwrap_or(usize::MAX),
    };
    let streamed = native.operation == Operation::StreamGenerateContent
        || headers
            .get(http::header::CONTENT_TYPE)
            .is_some_and(|v| v.as_bytes().starts_with(b"text/event-stream"));
    let body = if streamed {
        collect(native.dialect, body, limits, events, facts.namespace).await?
    } else {
        serde_json::from_slice(&read_http_body(body, limits).await.map_err(codec)?)?
    };
    let Converted {
        value,
        report: conversion,
    } = interactions::response(
        client,
        native,
        body,
        &facts.id,
        &facts.model,
        facts.created_at,
    )?;
    report.diagnostics.extend(conversion.diagnostics);
    for name in [
        http::header::CONTENT_LENGTH,
        http::header::CONTENT_ENCODING,
        http::header::TRANSFER_ENCODING,
    ] {
        headers.remove(name);
    }
    let body = if client.operation == Operation::StreamGenerateContent {
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );
        let chunks = synthesize(client.dialect, value, facts.namespace, limits, events)?;
        HttpBody::Stream(Box::pin(futures_util::stream::iter(
            chunks.into_iter().map(Ok),
        )))
    } else {
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        HttpBody::Bytes(encode_json(&value, limits).map_err(codec)?)
    };
    Ok(InteractionOutcome::Success {
        response: WireResponse {
            status,
            headers,
            body,
        },
        report,
    })
}

async fn collect(
    dialect: Dialect,
    body: HttpBody,
    limits: CodecLimits,
    events: EventLimits,
    namespace: IdNamespace,
) -> Result<Value, TransformError> {
    let mut reader = NativeReader::new(body, SourceFraming::Sse, limits);
    if dialect == Dialect::GeminiInteractions {
        let mut collector = InteractionStreamCollector::new(events.max_bytes);
        while let Some(frame) = reader.next::<Value>().await.map_err(codec)? {
            match frame {
                NativeFrame::Event { value, .. } => {
                    let terminal = value["event_type"] == "interaction.completed";
                    collector.push(serde_json::from_value(value)?)?;
                    if terminal {
                        break;
                    }
                }
                NativeFrame::Done => {
                    return Err(TransformError::invalid_result(
                        "interaction.stream",
                        "Interactions does not use DONE framing",
                    ));
                }
            }
        }
        return Ok(serde_json::to_value(collector.finish()?.value)?);
    }
    match dialect {
        Dialect::OpenAi => {
            collect_foreign::<r::stream::StreamEvent>(reader, events, namespace).await
        }
        Dialect::OpenAiChat => {
            collect_foreign::<h::stream::ChatCompletionChunk>(reader, events, namespace).await
        }
        Dialect::Claude => collect_foreign::<cs::StreamEvent>(reader, events, namespace).await,
        _ => Err(TransformError::unsupported(
            "interaction.collect",
            "unsupported upstream stream",
        )),
    }
}

async fn collect_foreign<E: NativeEvent>(
    mut reader: NativeReader,
    limits: EventLimits,
    namespace: IdNamespace,
) -> Result<Value, TransformError> {
    let mut collector = E::collector(
        IdentityFlow::new(namespace),
        TargetIdPolicy::new(E::DIALECT),
        limits,
    );
    while let Some(frame) = reader.next::<Value>().await.map_err(codec)? {
        match frame {
            NativeFrame::Event { value, .. } => {
                let event = E::decode(value)?;
                let terminal = event.is_terminal();
                E::collect(&mut collector, event)?;
                if terminal && !E::DONE {
                    break;
                }
            }
            NativeFrame::Done => {
                E::collect_done(&mut collector)?;
                break;
            }
        }
    }
    Ok(serde_json::to_value(E::collected(collector)?.value.value)?)
}

fn synthesize(
    dialect: Dialect,
    body: Value,
    namespace: IdNamespace,
    limits: CodecLimits,
    events: EventLimits,
) -> Result<Vec<Bytes>, TransformError> {
    if dialect == Dialect::GeminiInteractions {
        let events = synthesize_interaction_stream(
            serde_json::from_value::<i::Interaction>(body)?,
            events.max_bytes,
        )?
        .value;
        let mut chunks = Vec::new();
        let mut size = 0u64;
        for event in events {
            let value = serde_json::to_value(event)?;
            let data = encode_json(&value, limits).map_err(codec)?;
            let data = std::str::from_utf8(&data)
                .map_err(|e| TransformError::invalid_result("interaction.stream", e.to_string()))?;
            let bytes = encode_sse_event(
                &crate::codec::SseEvent {
                    event: value["event_type"].as_str().map(str::to_owned),
                    id: None,
                    retry: None,
                    data: data.to_owned(),
                },
                limits,
            )
            .map_err(codec)?;
            size = size.saturating_add(bytes.len() as u64);
            if size > limits.max_body_bytes {
                return Err(TransformError::new(
                    TransformErrorKind::Limit,
                    "interaction.stream",
                    "synthesis body exceeds limit",
                ));
            }
            chunks.push(bytes);
        }
        return Ok(chunks);
    }
    match dialect {
        Dialect::OpenAi => {
            synthesize_foreign::<r::GenerateContentResponseBody>(body, namespace, limits, events)
        }
        Dialect::OpenAiChat => {
            synthesize_foreign::<h::GenerateContentResponseBody>(body, namespace, limits, events)
        }
        Dialect::Claude => {
            synthesize_foreign::<c::GenerateContentResponseBody>(body, namespace, limits, events)
        }
        _ => Err(TransformError::unsupported(
            "interaction.synthesize",
            "unsupported client stream",
        )),
    }
}

fn synthesize_foreign<C: CompleteResponse + serde::de::DeserializeOwned>(
    body: Value,
    namespace: IdNamespace,
    limits: CodecLimits,
    events: EventLimits,
) -> Result<Vec<Bytes>, TransformError> {
    let response: C = serde_json::from_value(body)?;
    let events = response.events(namespace, events)?.value;
    let mut chunks = Vec::new();
    let mut size = 0u64;
    for event in events {
        let data = encode_json(&event, limits).map_err(codec)?;
        let data = std::str::from_utf8(&data)
            .map_err(|e| TransformError::invalid_result("generation.stream", e.to_string()))?;
        let bytes = encode_sse_event(
            &crate::codec::SseEvent {
                event: event.event_name().map(str::to_owned),
                id: None,
                retry: None,
                data: data.to_owned(),
            },
            limits,
        )
        .map_err(codec)?;
        size = size.saturating_add(bytes.len() as u64);
        if size > limits.max_body_bytes {
            return Err(TransformError::new(
                TransformErrorKind::Limit,
                "generation.stream",
                "synthesis body exceeds limit",
            ));
        }
        chunks.push(bytes);
    }
    if C::Event::DONE {
        chunks.push(encode_sse_done());
    }
    Ok(chunks)
}
