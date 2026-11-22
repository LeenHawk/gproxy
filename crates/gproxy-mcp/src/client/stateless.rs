//! Revision 2026-07-28: no `initialize`, no session. Each request names the
//! revision and the client in its own `_meta` and in its headers.

use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::{Answer, CallError, Context, Target, client_info, wire};

pub(crate) const VERSION: &str = "2026-07-28";

fn headers(method: &str, params: &Value) -> Result<HeaderMap, CallError> {
    let mut headers = HeaderMap::new();
    let mut add = |name, value: &str| -> Result<(), CallError> {
        let (name, value) = wire::header(name, value)?;
        headers.insert(name, value);
        Ok(())
    };
    add("mcp-protocol-version", VERSION)?;
    add("mcp-method", method)?;
    if method == "tools/call"
        && let Some(name) = params.get("name").and_then(Value::as_str)
    {
        add("mcp-name", name)?;
    }
    Ok(headers)
}

/// The request's own context, which this revision requires on every request.
fn with_context(mut params: Value, context: &Context<'_>) -> Value {
    if !params.is_object() {
        params = json!({});
    }
    let meta = &mut params["_meta"];
    if !meta.is_object() {
        *meta = json!({});
    }
    meta["io.modelcontextprotocol/protocolVersion"] = json!(VERSION);
    meta["io.modelcontextprotocol/clientInfo"] = client_info();
    meta["io.modelcontextprotocol/clientCapabilities"] =
        context.capabilities.cloned().unwrap_or_else(|| json!({}));
    params
}

pub(super) async fn request(
    target: Target<'_>,
    context: &Context<'_>,
    method: &str,
    params: Value,
) -> Result<Answer, CallError> {
    let params = with_context(params, context);
    let headers = headers(method, &params)?;
    wire::send(
        target,
        Method::POST,
        headers,
        Some(&wire::request_body(method, params)),
    )
    .await?
    .answer()
}

/// Whether the server speaks this revision: it answers `server/discover` and
/// lists the revision. Any other answer, an error or an HTTP refusal, means
/// it has to be opened with `initialize`.
pub(super) async fn supported(target: Target<'_>) -> Result<bool, CallError> {
    let answer = match request(
        target,
        &Context { capabilities: None },
        "server/discover",
        json!({}),
    )
    .await
    {
        Ok(answer) => answer,
        Err(CallError::Status(_) | CallError::Invalid(_)) => return Ok(false),
        Err(error) => return Err(error),
    };
    Ok(answer.is_ok_and(|result| {
        result["supportedVersions"]
            .as_array()
            .is_some_and(|versions| versions.iter().any(|version| version == VERSION))
    }))
}
