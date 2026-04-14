use crate::response::UpstreamError;

/// Wrap a Gemini request body in the Code Assist (v1internal) envelope.
///
/// Input: standard Gemini JSON body (with optional top-level `model` field,
/// which is stripped because the outer envelope carries it).
/// Output: `{"model": "<name>", "project": "<project_id>",
///           "user_prompt_id": "<random hex>", "request": {<gemini body>}}`
///
/// The `user_prompt_id` field is required by the Code Assist API — without
/// it, `/v1internal:streamGenerateContent` returns
/// `INVALID_ARGUMENT: Request contains an invalid argument`. Every wrap
/// generates a fresh 16-byte random id so upstream telemetry can correlate
/// attempts.
///
/// The inner body's `contents[].role` field is also forced to `"user"`
/// when missing. The public `generativelanguage.googleapis.com` API
/// defaults missing roles to `user` server-side, but Code Assist
/// (`cloudcode-pa.googleapis.com`) is strictly typed and rejects
/// content blocks without an explicit role — again with the generic
/// `INVALID_ARGUMENT: Request contains an invalid argument`. Injecting
/// the default here lets clients continue sending the looser
/// public-API shape to `/geminicli/...` routes.
pub fn wrap_request(
    body: &[u8],
    model: Option<&str>,
    project_id: &str,
) -> Result<Vec<u8>, UpstreamError> {
    let mut inner: serde_json::Value = serde_json::from_slice(body)
        .map_err(|e| UpstreamError::RequestBuild(format!("json parse for envelope wrap: {e}")))?;

    // Extract model from body if present, otherwise use the provided model.
    let model_name = inner
        .as_object_mut()
        .and_then(|obj| obj.remove("model"))
        .and_then(|v| v.as_str().map(String::from))
        .or_else(|| model.map(String::from))
        .unwrap_or_default();

    ensure_content_roles(&mut inner);

    let envelope = serde_json::json!({
        "model": model_name,
        "project": project_id,
        "user_prompt_id": generate_user_prompt_id(),
        "request": inner,
    });

    serde_json::to_vec(&envelope)
        .map_err(|e| UpstreamError::RequestBuild(format!("envelope serialize: {e}")))
}

/// Inject `"role": "user"` on any top-level `contents[]` block that
/// doesn't already carry a role. Code Assist (v1internal) rejects
/// content blocks without an explicit role, even though the public
/// Gemini API allows omitting it.
fn ensure_content_roles(body: &mut serde_json::Value) {
    let Some(contents) = body
        .as_object_mut()
        .and_then(|obj| obj.get_mut("contents"))
        .and_then(|v| v.as_array_mut())
    else {
        return;
    };
    for content in contents.iter_mut() {
        let Some(obj) = content.as_object_mut() else {
            continue;
        };
        if !obj.contains_key("role") {
            obj.insert(
                "role".to_string(),
                serde_json::Value::String("user".to_string()),
            );
        }
    }
}

fn generate_user_prompt_id() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Unwrap a Code Assist API response envelope.
///
/// Input: `{"response": { <gemini response> }, "traceId": "..."}`
/// Output: the inner `<gemini response>` object as bytes.
///
/// If parsing fails or no `"response"` key is found, the body is returned as-is.
pub fn unwrap_response(body: &[u8]) -> Vec<u8> {
    let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(body) else {
        return body.to_vec();
    };

    if let Some(inner) = json.as_object_mut().and_then(|obj| obj.remove("response")) {
        serde_json::to_vec(&inner).unwrap_or_else(|_| body.to_vec())
    } else {
        body.to_vec()
    }
}
