//! Google Code Assist envelope (shared by `geminicli` + `antigravity`).
//!
//! Code Assist wraps the standard Gemini `generateContent` API:
//!   * **Request** — a gemini body is nested under `request`, alongside routing
//!     metadata: `{model, project, user_prompt_id, request:<body>}`. Code Assist
//!     additionally requires every `contents[].role` to be present, so a missing
//!     role is forced to `"user"`.
//!   * **Response** — the gemini payload is nested under `.response`; unwrap it.
//!   * **Stream** — each SSE frame's `data:` is a `{"response":{…}}` object;
//!     unwrap the inner payload per frame so the downstream M2 transform sees
//!     canonical gemini `alt=sse` frames.
//!
//! All helpers are total: a body that does not match the envelope shape is
//! returned unchanged (Code Assist is the only producer, but tolerance keeps a
//! mis-routed body from being corrupted).

use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue, USER_AGENT};
use http::{HeaderMap, Method, Request, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::channel::http_util::{build_request, join_url};
use crate::channel::usage::{UsageSnapshot, UsageWindow};
use crate::channel::{ChannelError, ChannelStreamDecoder};
use crate::transform::common::sse::SseDecoder;

/// A fresh random `user_prompt_id` (16 bytes → 32 hex chars). Code Assist treats
/// it as an opaque per-request id; shared by `geminicli` + `antigravity`.
/// Randomness comes from `crate::util::rand` — one cross-target source
/// (compiles on wasm without uuid's native-only gate).
pub fn random_user_prompt_id() -> String {
    let bytes = crate::util::rand::bytes::<16>();
    let mut out = String::with_capacity(32);
    for b in bytes {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        out.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    out
}

/// Wrap a gemini `generateContent` body in the Code Assist request envelope.
/// `user_prompt_id` is caller-generated (random hex). Forces a missing
/// `contents[].role` to `"user"` (Code Assist rejects role-less turns).
pub fn wrap_code_assist(
    gemini_body: &[u8],
    model: &str,
    project: &str,
    user_prompt_id: &str,
) -> Result<Vec<u8>, ChannelError> {
    let mut request: Value = serde_json::from_slice(gemini_body)
        .map_err(|e| ChannelError::Build(format!("gemini body is not JSON: {e}")))?;
    force_user_roles(&mut request);
    let envelope = json!({
        "model": model,
        "project": project,
        "user_prompt_id": user_prompt_id,
        "request": request,
    });
    serde_json::to_vec(&envelope)
        .map_err(|e| ChannelError::Build(format!("serialize code-assist envelope: {e}")))
}

/// Build the Code Assist `:countTokens` body. The count endpoint differs from
/// generate: it rejects the top-level `model`/`project`/`user_prompt_id`
/// envelope fields and wants only `request` as a plain `GenerateContentRequest`
/// (NOT a CountTokensRequest, so no `generateContentRequest` wrapper). Mirrors
/// CLIProxyAPI's gemini-cli/antigravity count path.
pub fn wrap_code_assist_count(gemini_count_body: &[u8]) -> Result<Vec<u8>, ChannelError> {
    let parsed: Value = serde_json::from_slice(gemini_count_body)
        .map_err(|e| ChannelError::Build(format!("gemini count body is not JSON: {e}")))?;
    // The CountTokens transform nests the real GenerateContentRequest under
    // `generateContentRequest`; lift it up to be `request`. Fall back to a bare
    // `{contents}` if a caller sent contents directly.
    let mut request = parsed
        .get("generateContentRequest")
        .cloned()
        .unwrap_or_else(
            || json!({ "contents": parsed.get("contents").cloned().unwrap_or_else(|| json!([])) }),
        );
    force_user_roles(&mut request);
    let envelope = json!({ "request": request });
    serde_json::to_vec(&envelope)
        .map_err(|e| ChannelError::Build(format!("serialize code-assist count envelope: {e}")))
}

/// Unwrap a Code Assist response: extract `.response`. On a parse failure or a
/// missing `.response` the original body is returned unchanged.
pub fn unwrap_code_assist(body: Bytes) -> Bytes {
    match unwrap_response_bytes(&body) {
        Some(inner) => Bytes::from(inner),
        None => body,
    }
}

/// Set `contents[].role = "user"` for any turn missing a role.
fn force_user_roles(request: &mut Value) {
    if let Some(contents) = request.get_mut("contents").and_then(Value::as_array_mut) {
        for turn in contents {
            if let Some(obj) = turn.as_object_mut()
                && !obj.contains_key("role")
            {
                obj.insert("role".to_string(), Value::String("user".to_string()));
            }
        }
    }
}

/// Parse `raw` and reserialize its `.response`; `None` if it does not parse or
/// has no `.response` field.
fn unwrap_response_bytes(raw: &[u8]) -> Option<Vec<u8>> {
    let v: Value = serde_json::from_slice(raw).ok()?;
    let inner = v.get("response")?;
    serde_json::to_vec(inner).ok()
}

/// Per-frame Code Assist stream unwrap (envelope SSE → canonical gemini SSE).
/// Decodes upstream SSE frames, extracts `.response` from each `data:` payload,
/// and re-emits `data: {inner}\n\n`. A frame whose data is not an envelope is
/// passed through verbatim (re-encoded as a `data:` frame).
#[derive(Default)]
pub struct CodeAssistStreamDecoder {
    decoder: SseDecoder,
}

impl CodeAssistStreamDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    fn emit(data: &str, out: &mut Vec<u8>) {
        let unwrapped = unwrap_response_bytes(data.as_bytes());
        let payload = match &unwrapped {
            Some(inner) => std::str::from_utf8(inner).unwrap_or(data),
            None => data,
        };
        out.extend_from_slice(b"data: ");
        out.extend_from_slice(payload.as_bytes());
        out.extend_from_slice(b"\n\n");
    }
}

impl ChannelStreamDecoder for CodeAssistStreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for frame in self.decoder.push(chunk) {
            Self::emit(&frame.data, &mut out);
        }
        out
    }

    fn finish(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(frame) = self.decoder.finish() {
            Self::emit(&frame.data, &mut out);
        }
        out
    }
}

/// Build `POST {base}/v1internal:retrieveUserQuota` with `{"project":<id>}` and
/// the given Code Assist User-Agent. Shared by `geminicli` + `antigravity` (the
/// per-credential usage query); the response is parsed by [`parse_user_quota`].
pub fn user_quota_request(
    base: &str,
    access_token: &str,
    project_id: &str,
    user_agent: &str,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let body = serde_json::to_vec(&json!({ "project": project_id }))
        .map_err(|e| ChannelError::Build(e.to_string()))?;
    let uri = join_url(base, "/v1internal:retrieveUserQuota", None)?;
    let mut req = build_request(Method::POST, uri, HeaderMap::new(), Bytes::from(body))?;
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
    let ua = HeaderValue::from_str(user_agent)
        .map_err(|e| ChannelError::Build(format!("bad user-agent: {e}")))?;
    let h = req.headers_mut();
    h.insert(AUTHORIZATION, bearer);
    h.insert(ACCEPT, HeaderValue::from_static("application/json"));
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h.insert(USER_AGENT, ua);
    Ok(Some(req))
}

/// Parse a Code Assist `retrieveUserQuota` response (`{"buckets":[…]}`) into a
/// snapshot: one window per bucket, `used_percent` derived from
/// `remainingFraction`, the ISO `resetTime` kept verbatim.
pub fn parse_user_quota(status: StatusCode, body: &Bytes) -> Option<UsageSnapshot> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    let resp: RetrieveUserQuotaResponse = serde_json::from_value(raw.clone()).ok()?;
    let windows = resp
        .buckets
        .iter()
        .enumerate()
        .map(|(i, b)| b.to_window(i))
        .collect();
    Some(UsageSnapshot {
        plan: None,
        windows,
        credits: None,
        rate_limit_reset_credits: None,
        raw,
    })
}

#[derive(Deserialize)]
struct RetrieveUserQuotaResponse {
    #[serde(default)]
    buckets: Vec<BucketInfo>,
}

/// One per-model quota bucket. `remainingFraction` is the fraction LEFT [0,1];
/// `resetTime` an ISO-8601 timestamp.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BucketInfo {
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    remaining_fraction: Option<f64>,
    #[serde(default)]
    reset_time: Option<String>,
}

impl BucketInfo {
    fn to_window(&self, i: usize) -> UsageWindow {
        let name = self
            .model_id
            .clone()
            .or_else(|| self.token_type.clone())
            .unwrap_or_else(|| format!("bucket_{i}"));
        let used_percent = self
            .remaining_fraction
            .map(|f| ((1.0 - f) * 100.0).clamp(0.0, 100.0));
        let mut w = UsageWindow {
            name,
            used_percent,
            ..Default::default()
        };
        if let Some(rt) = &self.reset_time {
            w = w.resets_iso(rt.clone());
        }
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_assist_roundtrip() {
        // wrap: envelope carries the four fields; role defaulted to "user".
        let body = br#"{"contents":[{"parts":[{"text":"hi"}]}]}"#;
        let wrapped = wrap_code_assist(body, "gemini-2.0", "proj-1", "pid-abc").unwrap();
        let v: Value = serde_json::from_slice(&wrapped).unwrap();
        assert_eq!(v["model"], "gemini-2.0");
        assert_eq!(v["project"], "proj-1");
        assert_eq!(v["user_prompt_id"], "pid-abc");
        assert_eq!(v["request"]["contents"][0]["role"], "user");
        assert_eq!(v["request"]["contents"][0]["parts"][0]["text"], "hi");

        // unwrap: `.response` extracted.
        let enveloped = Bytes::from(r#"{"response":{"candidates":[{"x":1}]}}"#);
        let inner: Value = serde_json::from_slice(&unwrap_code_assist(enveloped)).unwrap();
        assert_eq!(inner["candidates"][0]["x"], 1);

        // unwrap: non-envelope body returned unchanged.
        let plain = Bytes::from(r#"{"candidates":[]}"#);
        assert_eq!(unwrap_code_assist(plain.clone()), plain);

        // stream: per-frame `.response` unwrap.
        let mut dec = CodeAssistStreamDecoder::new();
        let mut out = dec.push(b"data: {\"response\":{\"t\":\"a\"}}\n\n");
        out.extend(dec.finish());
        assert_eq!(String::from_utf8(out).unwrap(), "data: {\"t\":\"a\"}\n\n");
    }

    #[test]
    fn count_envelope_strips_to_bare_request() {
        // The CountTokens transform nests the real GenerateContentRequest under
        // `generateContentRequest`; the count envelope lifts it to `request` and
        // drops model/project/user_prompt_id (which Code Assist `:countTokens`
        // rejects with "Unknown name ...").
        let body = br#"{"model":"gemini-2.5-flash","contents":[],"generateContentRequest":{"contents":[{"parts":[{"text":"hi"}]}]}}"#;
        let v: Value = serde_json::from_slice(&wrap_code_assist_count(body).unwrap()).unwrap();
        assert!(v.get("model").is_none() && v.get("project").is_none());
        assert!(v.get("user_prompt_id").is_none() && v.get("generateContentRequest").is_none());
        assert_eq!(v["request"]["contents"][0]["role"], "user");
        assert_eq!(v["request"]["contents"][0]["parts"][0]["text"], "hi");
    }

    #[test]
    fn parses_user_quota_buckets() {
        let body = Bytes::from_static(
            br#"{"buckets":[
              {"modelId":"gemini-2.5-pro","tokenType":"REQUESTS","remainingFraction":0.75,
               "resetTime":"2026-06-22T16:01:15Z"}
            ]}"#,
        );
        let snap = parse_user_quota(StatusCode::OK, &body).expect("snapshot");
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].name, "gemini-2.5-pro");
        // remainingFraction 0.75 → 25% used.
        assert_eq!(snap.windows[0].used_percent, Some(25.0));
        assert_eq!(
            snap.windows[0].resets_at.as_deref(),
            Some("2026-06-22T16:01:15Z")
        );
    }
}
