//! Codex CLI retries ServerOverloaded and RetryLimit only with server advice.
//! HTTP refusals carry it on the response, failed turns inside the error.
use futures_util::{StreamExt, stream};
use gproxy_protocol::{
    HttpBody, WireResponse,
    connection::{Bytes, WsFrame},
};
use http::{StatusCode, header};
use serde_json::{Value, json};

fn eligible(status: u16, error: &Value) -> bool {
    let code = error["code"].as_str();
    if status == 503 {
        return code == Some("server_is_overloaded");
    }
    // api_bridge.rs gives these 429s their own terminal errors. All other
    // 429s become RetryLimit, including an empty or non-JSON response.
    status == 429
        && !matches!(
            error["type"].as_str(),
            Some("usage_limit_reached" | "usage_not_included" | "insufficient_quota")
        )
        && !matches!(
            code,
            Some(
                "usage_limit_reached"
                    | "usage_not_included"
                    | "flex_unavailable"
                    | "insufficient_quota"
                    | "credit_balance_exhausted"
                    | "organization_spend_limit_exceeded"
                    | "project_spend_limit_exceeded"
                    | "organization_usage_limit_exceeded"
            )
        )
}

pub(super) async fn advise_http(response: &mut WireResponse) {
    if response.headers.contains_key(header::RETRY_AFTER)
        || !matches!(
            response.status,
            StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE
        )
    {
        return;
    }
    if let HttpBody::Stream(rest) = &mut response.body {
        let mut bytes = Vec::new();
        while let Some(chunk) = rest.next().await {
            match chunk {
                Ok(chunk) => bytes.extend_from_slice(&chunk),
                Err(error) => {
                    // Leave the original refusal, including its transfer error, readable.
                    let head = stream::iter([Ok(Bytes::from(bytes)), Err(error)]);
                    let rest = std::mem::replace(rest, Box::pin(stream::empty()));
                    response.body = HttpBody::Stream(Box::pin(head.chain(rest)));
                    return;
                }
            }
        }
        response.body = HttpBody::Bytes(bytes.into());
    }
    if let HttpBody::Bytes(bytes) = &response.body {
        let value = serde_json::from_slice::<Value>(bytes).unwrap_or_default();
        if eligible(response.status.as_u16(), &value["error"]) {
            response
                .headers
                .insert(header::RETRY_AFTER, http::HeaderValue::from_static("0"));
        }
    }
}

fn has_advice(headers: &Value) -> bool {
    headers.as_object().is_some_and(|headers| {
        headers
            .keys()
            .any(|key| key.eq_ignore_ascii_case("retry-after"))
    })
}

/// Preserve the CLI's nested-before-outer precedence for wrapped WS errors.
/// For response.failed the CLI reads only response.error.headers.
pub(super) fn event(data: &[u8], fallback: &str) -> Option<String> {
    let mut value: Value = serde_json::from_slice(data).ok()?;
    let path = match value["type"].as_str()? {
        "response.failed"
            if value.pointer("/response/error/code")?.as_str()? == "server_is_overloaded" =>
        {
            "/response/error"
        }
        "error" => {
            let status = value
                .get("status")
                .or_else(|| value.get("status_code"))?
                .as_u64()?;
            if !eligible(u16::try_from(status).ok()?, &value["error"])
                || has_advice(&value["error"]["headers"])
            {
                return None;
            }
            // A status-only rejection may have no nested error.
            ""
        }
        _ => return None,
    };
    let envelope = value.pointer_mut(path)?.as_object_mut()?;
    let headers = envelope.entry("headers").or_insert(Value::Null);
    if headers.is_null() {
        *headers = json!({});
    }
    if has_advice(headers) {
        return None;
    }
    headers
        .as_object_mut()?
        .insert("retry-after".into(), fallback.into());
    Some(value.to_string())
}

pub(super) fn frame(frame: WsFrame) -> WsFrame {
    match &frame {
        WsFrame::Text(text) => event(text.as_bytes(), "0").map(WsFrame::Text),
        WsFrame::Binary(bytes) => event(bytes, "0").map(|text| WsFrame::Binary(text.into())),
        _ => None,
    }
    .unwrap_or(frame)
}
