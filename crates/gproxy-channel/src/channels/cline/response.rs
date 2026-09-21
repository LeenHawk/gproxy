//! The two response shapes Cline answers with, rewritten into the dialect
//! the host was promised.
//!
//! Cline's REST surface wraps a successful reply in `{"success": true,
//! "data": ...}` and reports a failure as `{"success": false, "error": ...}`
//! (v3 `cline/response.rs`). A buffered Chat Completions reply therefore has
//! to be unwrapped before it is a Chat Completions reply at all. The
//! streamed reply is ordinary SSE and carries no envelope, so the stream is
//! forwarded untouched.
//!
//! The catalogue is not a model list either: `GET
//! /ai/cline/recommended-models` answers `{"free": [...], "clinePass": [...]}`
//! with the same model sometimes in both groups. It becomes an OpenAI model
//! list, each entry keeping the group it was first seen in under
//! `cline_group` (v3 `cline/response.rs::model_list`).

use super::Cline;
use crate::channel::{BaseChannel, ChannelError, OperationContext, PrepareContext};
use crate::channels::shared::compatible::http::{invalid_response, read_body};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{HttpBody, Operation, OperationKey, WireResponse};
use http::header;
use serde_json::{Map, Value};

/// The model groups the catalogue reports, in the order they are offered.
const GROUPS: [&str; 2] = ["free", "clinePass"];

fn encode(value: &Value) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(value)
        .map(Bytes::from)
        .map_err(|error| invalid_response(format!("{}: {error}", super::ID)))
}

/// The payload inside the envelope. A body that is not an envelope (an error
/// document, or a reply the upstream sent unwrapped) is returned as it came.
pub(super) fn unwrap_envelope(body: &[u8]) -> Result<Option<Value>, ChannelError> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Ok(None);
    };
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return Ok(None);
    }
    match value.get("data") {
        Some(data) => Ok(Some(data.clone())),
        None => Err(invalid_response(format!(
            "{}: a successful envelope with no data",
            super::ID
        ))),
    }
}

/// `{"free": [...], "clinePass": [...]}` as an OpenAI model list.
pub(super) fn catalog(body: &[u8]) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| invalid_response(format!("{} catalogue: {error}", super::ID)))?;
    // An error document is the upstream's answer, not a catalogue to rewrite.
    if value.get("error").is_some() {
        return Ok(Bytes::copy_from_slice(body));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut data = Vec::new();
    for group in GROUPS {
        for model in value
            .get(group)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(mut model) = model.as_object().cloned() else {
                continue;
            };
            let Some(id) = model.get("id").and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            if !seen.insert(id) {
                continue;
            }
            model.insert("object".into(), Value::String("model".into()));
            model.insert("cline_group".into(), Value::String(group.into()));
            data.push(Value::Object(model));
        }
    }
    let mut out = Map::new();
    out.insert("object".into(), Value::String("list".into()));
    out.insert("data".into(), Value::Array(data));
    encode(&Value::Object(out))
}

/// Replace a rewritten body, dropping the length the old one stated.
fn replace(response: &mut WireResponse<HttpBody>, body: Bytes) {
    response.headers.remove(header::CONTENT_LENGTH);
    response.headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    response.body = HttpBody::Bytes(body);
}

/// Send one operation and rewrite whatever the upstream wrapped it in. A
/// non-2xx reply is the upstream's own answer and is returned untouched.
pub(super) async fn invoke(
    channel: &Cline,
    operation: Operation,
    ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let OperationContext {
        provider,
        credential,
        dialect,
        request,
        client,
        endpoint_override,
        ..
    } = ctx;
    let prepared = channel.prepare(PrepareContext {
        provider,
        credential,
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override,
    })?;
    let mut response = client.send(prepared).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let bytes = read_body(std::mem::replace(
        &mut response.body,
        HttpBody::Bytes(Bytes::new()),
    ))
    .await?;
    let rewritten = match operation {
        Operation::ListModels => catalog(&bytes)?,
        _ => match unwrap_envelope(&bytes)? {
            Some(data) => encode(&data)?,
            None => bytes,
        },
    };
    replace(&mut response, rewritten);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_keeps_the_group_a_model_was_first_offered_in() {
        let raw = br#"{"free":[{"id":"a/model"}],
                       "clinePass":[{"id":"a/model"},{"id":"b/model"}]}"#;
        let list: Value = serde_json::from_slice(&catalog(raw).unwrap()).unwrap();
        assert_eq!(list["object"], "list");
        let data = list["data"].as_array().unwrap();
        assert_eq!(data.len(), 2, "a model offered twice is listed once");
        assert_eq!(data[0]["cline_group"], "free");
        assert_eq!(data[1]["cline_group"], "clinePass");
        assert_eq!(data[0]["object"], "model");
    }

    #[test]
    fn only_a_successful_envelope_is_unwrapped() {
        let wrapped = br#"{"success":true,"data":{"id":"gen-1"}}"#;
        assert_eq!(unwrap_envelope(wrapped).unwrap().unwrap()["id"], "gen-1");
        let failed = br#"{"success":false,"error":"no"}"#;
        assert!(unwrap_envelope(failed).unwrap().is_none());
        let bare = br#"{"id":"gen-1"}"#;
        assert!(unwrap_envelope(bare).unwrap().is_none());
        assert!(unwrap_envelope(b"not json").unwrap().is_none());
        assert!(unwrap_envelope(br#"{"success":true}"#).is_err());
    }
}
