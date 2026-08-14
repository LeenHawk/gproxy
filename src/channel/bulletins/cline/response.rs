use bytes::Bytes;
use serde_json::Value;

/// Cline buffers Chat completions inside `{ "success": true, "data": ... }`,
/// while its streaming endpoint emits canonical OpenAI SSE directly. Only
/// unwrap the positive JSON envelope; malformed bodies, SSE, and error shapes
/// pass through unchanged so the pipeline can preserve upstream diagnostics.
pub(super) fn unwrap_chat(body: Bytes) -> Bytes {
    let Some(data) = serde_json::from_slice::<Value>(&body)
        .ok()
        .filter(|value| value.get("success").and_then(Value::as_bool) == Some(true))
        .and_then(|value| value.get("data").cloned())
    else {
        return body;
    };
    serde_json::to_vec(&data).map(Bytes::from).unwrap_or(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_buffered_chat_completion() {
        let body = Bytes::from_static(
            br#"{"success":true,"data":{"id":"gen_1","object":"chat.completion","choices":[]}}"#,
        );
        let output: Value = serde_json::from_slice(&unwrap_chat(body)).unwrap();
        assert_eq!(output["id"], "gen_1");
        assert_eq!(output["object"], "chat.completion");
        assert!(output.get("data").is_none());
        assert!(output.get("success").is_none());
    }

    #[test]
    fn preserves_streams_and_error_envelopes() {
        let stream = Bytes::from_static(
            b"data: {\"id\":\"gen_1\",\"object\":\"chat.completion.chunk\"}\n\n",
        );
        assert_eq!(unwrap_chat(stream.clone()), stream);

        let error = Bytes::from_static(br#"{"success":false,"error":"denied"}"#);
        assert_eq!(unwrap_chat(error.clone()), error);
    }
}
