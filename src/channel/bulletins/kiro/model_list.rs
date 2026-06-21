//! Kiro model-list (整形) — Kiro has no OpenAI-compatible `/v1/models`. Captured
//! from the real Kiro CLI, the catalogue is the AWS-JSON Smithy operation
//! `AmazonCodeWhispererService.ListAvailableModels` on the **management** host
//! (`POST https://management.{region}.kiro.dev/`), NOT v1's old Amazon Q
//! `GET …/ListAvailableModels?origin=AI_EDITOR` (a different product).
//!
//!   * [`request`] builds that POST (origin=KIRO_CLI + profileArn in query AND
//!     body, `application/x-amz-json-1.0`, `x-amz-target`, Bearer).
//!   * [`to_openai`] reprojects the `{"models":[{"modelId"}]}` response into the
//!     OpenAI canonical model-list wire shape so `parse_models` reads `data[].id`.
//!
//! Best-effort: the ORIGINAL bytes are returned on parse failure or when the
//! body is not in the expected shape.

use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use http::{HeaderMap, Method, Request};
use serde_json::{Value, json};

use super::{AMZ_JSON, ORIGIN, TARGET_LIST_MODELS, UA_MANAGEMENT, auth, management_base};
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, join_url};

/// True when this inbound request is the family model-list request: the admin
/// model-pull sends `GET /v1/models` (OpenAi family). Detect it from method +
/// path so the content path is untouched.
pub(super) fn is_model_list(method: &Method, path: &str) -> bool {
    method == Method::GET && path.trim_end_matches('/').ends_with("/models")
}

/// Build the Smithy `ListAvailableModels` request captured from the Kiro CLI:
/// `POST https://management.{region}.kiro.dev/?origin=KIRO_CLI&profileArn=…`,
/// `content-type: application/x-amz-json-1.0`, body `{"origin","profileArn"}`,
/// Bearer auth. (v1's `GET …/ListAvailableModels?origin=AI_EDITOR` was the OLD
/// Amazon Q product.) Response: `{"models":[{"modelId"}]}`.
pub(super) fn request(secret: &Value, settings: &Value) -> Result<Request<Bytes>, ChannelError> {
    let access_token = auth::access_token(secret)?;
    // The real request always carries profileArn (in BOTH the query and body).
    let profile_arn = auth::profile_arn(secret, settings).ok_or_else(|| {
        ChannelError::InvalidCredential("kiro model-list requires a profileArn".into())
    })?;

    let path = format!("/?origin={ORIGIN}&profileArn={}", pct(profile_arn));
    let uri = join_url(&management_base(settings), &path, None)?;
    let body = serde_json::to_vec(&json!({ "origin": ORIGIN, "profileArn": profile_arn }))
        .map_err(|e| ChannelError::Build(format!("kiro model-list body: {e}")))?;

    let mut req = build_request(Method::POST, uri, HeaderMap::new(), Bytes::from(body))?;
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
    let invocation = HeaderValue::from_str(&crate::util::rand::uuid_v4())
        .map_err(|e| ChannelError::Build(format!("bad invocation id: {e}")))?;
    let h = req.headers_mut();
    h.insert(AUTHORIZATION, bearer);
    h.insert(ACCEPT, HeaderValue::from_static("*/*"));
    h.insert(CONTENT_TYPE, HeaderValue::from_static(AMZ_JSON));
    h.insert(USER_AGENT, HeaderValue::from_static(UA_MANAGEMENT));
    h.insert(
        HeaderName::from_static("x-amz-user-agent"),
        HeaderValue::from_static(UA_MANAGEMENT),
    );
    h.insert(
        HeaderName::from_static("x-amz-target"),
        HeaderValue::from_static(TARGET_LIST_MODELS),
    );
    h.insert(
        HeaderName::from_static("x-amzn-codewhisperer-optout"),
        HeaderValue::from_static("false"),
    );
    h.insert(
        HeaderName::from_static("amz-sdk-request"),
        HeaderValue::from_static("attempt=1; max=3"),
    );
    h.insert(HeaderName::from_static("amz-sdk-invocation-id"), invocation);
    Ok(req)
}

/// Reproject the Kiro `ListAvailableModels` body into the OpenAI canonical list
/// envelope `{"object":"list","data":[{id, object:"model", created, owned_by:
/// "amazon"}]}`. Mirrors v1 `kiro_model_list_to_openai_model_list`. Returns the
/// input unchanged on parse failure or when there is no `models`/`data` array.
pub(super) fn to_openai(body: Bytes) -> Bytes {
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(models) = payload
        .get("models")
        .or_else(|| payload.get("data"))
        .and_then(Value::as_array)
    else {
        return body;
    };

    let created = crate::util::time::unix_now();
    let data: Vec<Value> = models
        .iter()
        .filter_map(model_id)
        .map(|id| {
            json!({
                "id": id,
                "object": "model",
                "created": created,
                "owned_by": "amazon",
            })
        })
        .collect();

    match serde_json::to_vec(&json!({ "object": "list", "data": data })) {
        Ok(out) => Bytes::from(out),
        Err(_) => body,
    }
}

/// Pull a model id from a Kiro list entry. Entries are objects keyed by
/// `modelId`/`model_id`/`id`/`name`; a bare string is taken verbatim (v1 parity).
fn model_id(value: &Value) -> Option<String> {
    match value {
        Value::String(id) => Some(id.clone()),
        Value::Object(_) => value
            .get("modelId")
            .or_else(|| value.get("model_id"))
            .or_else(|| value.get("id"))
            .or_else(|| value.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

/// Percent-encode a query value, leaving the RFC 3986 unreserved set verbatim
/// (`profileArn` carries `:` and `/`). Matches `usage::pct`.
fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(
                char::from_digit((b >> 4) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
            out.push(
                char::from_digit((b & 0xf) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_list_available_models_request() {
        let secret = json!({
            "access_token": "kiro-token",
            "profile_arn": "arn:aws:codewhisperer:us-east-1:1:profile/x",
        });
        let settings = json!({});
        let req = request(&secret, &settings).unwrap();

        assert_eq!(req.method(), Method::POST);
        // Smithy ListAvailableModels on the management host; origin+profileArn in
        // the query AND body (captured from the real Kiro CLI).
        assert_eq!(
            req.uri().to_string(),
            "https://management.us-east-1.kiro.dev/?origin=KIRO_CLI\
             &profileArn=arn%3Aaws%3Acodewhisperer%3Aus-east-1%3A1%3Aprofile%2Fx"
        );
        let body: Value = serde_json::from_slice(req.body()).unwrap();
        assert_eq!(
            body,
            json!({"origin":"KIRO_CLI","profileArn":"arn:aws:codewhisperer:us-east-1:1:profile/x"})
        );
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer kiro-token"
        );
        assert_eq!(
            req.headers().get("x-amz-target").unwrap(),
            "AmazonCodeWhispererService.ListAvailableModels"
        );
        assert_eq!(
            req.headers().get("content-type").unwrap(),
            "application/x-amz-json-1.0"
        );
        assert_eq!(
            req.headers().get("x-amzn-codewhisperer-optout").unwrap(),
            "false"
        );
        assert!(req.headers().get("amz-sdk-invocation-id").is_some());
    }

    #[test]
    fn requires_profile_arn() {
        // profileArn is mandatory for the kiro.dev management API.
        assert!(request(&json!({ "access_token": "t" }), &json!({})).is_err());
    }

    #[test]
    fn reshapes_kiro_list_to_openai() {
        // v1-shaped response: `models` entries keyed by modelId/name; bare ids and
        // entries lacking any id key are handled (the latter dropped).
        let body = Bytes::from_static(
            br#"{"models":[{"modelId":"claude-sonnet-4.5"},{"name":"amazonq"},{"foo":"bar"}]}"#,
        );
        let out = to_openai(body);
        let v: Value = serde_json::from_slice(&out).unwrap();

        assert_eq!(v["object"], "list");
        let data = v["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["id"], "claude-sonnet-4.5");
        assert_eq!(data[0]["object"], "model");
        assert_eq!(data[0]["owned_by"], "amazon");
        assert!(data[0]["created"].is_number());
        assert_eq!(data[1]["id"], "amazonq");
    }

    #[test]
    fn reshape_passthrough_on_bad_input() {
        // No `models`/`data` array → returned verbatim.
        let body = Bytes::from_static(br#"{"unexpected":true}"#);
        assert_eq!(to_openai(body.clone()), body);

        // Non-JSON → returned verbatim.
        let bad = Bytes::from_static(b"not json");
        assert_eq!(to_openai(bad.clone()), bad);
    }
}
