//! Vertex/VertexExpress response normalization to AI-Studio (standard Gemini)
//! shape.

use bytes::Bytes;
use serde_json::Value;

/// Normalize Vertex/VertexExpress responses to match AI Studio (standard
/// Gemini) format.
///
/// - `candidates[].citationMetadata.citations` → `citationMetadata.citationSources`
/// - `promptFeedback.blockReason` value `BLOCKED_REASON_UNSPECIFIED` →
///   `BLOCK_REASON_UNSPECIFIED`
///
/// Best-effort: returns the input unchanged on JSON parse failure (or if a
/// changed value fails to re-serialize).
pub fn normalize_vertex_response(body: Bytes) -> Bytes {
    let Ok(mut json) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };

    let mut changed = false;

    if let Some(candidates) = json.get_mut("candidates").and_then(Value::as_array_mut) {
        for candidate in candidates {
            if let Some(cm) = candidate.get_mut("citationMetadata")
                && let Some(citations) = cm.as_object_mut().and_then(|m| m.remove("citations"))
            {
                cm.as_object_mut()
                    .unwrap()
                    .insert("citationSources".to_string(), citations);
                changed = true;
            }
        }
    }

    if let Some(pf) = json.get_mut("promptFeedback")
        && let Some(br) = pf.get_mut("blockReason")
        && br.as_str() == Some("BLOCKED_REASON_UNSPECIFIED")
    {
        *br = Value::String("BLOCK_REASON_UNSPECIFIED".to_string());
        changed = true;
    }

    if changed {
        match serde_json::to_vec(&json) {
            Ok(bytes) => Bytes::from(bytes),
            Err(_) => body,
        }
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renames_citations_and_block_reason() {
        let body = Bytes::from(
            json!({
                "candidates": [
                    {"citationMetadata": {"citations": [{"uri": "x"}]}}
                ],
                "promptFeedback": {"blockReason": "BLOCKED_REASON_UNSPECIFIED"}
            })
            .to_string(),
        );
        let out = normalize_vertex_response(body);
        let parsed: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            parsed["candidates"][0]["citationMetadata"]["citationSources"][0]["uri"],
            "x"
        );
        assert!(
            parsed["candidates"][0]["citationMetadata"]
                .get("citations")
                .is_none()
        );
        assert_eq!(
            parsed["promptFeedback"]["blockReason"],
            "BLOCK_REASON_UNSPECIFIED"
        );
    }

    #[test]
    fn returns_original_on_parse_failure() {
        let body = Bytes::from_static(b"not json");
        let out = normalize_vertex_response(body.clone());
        assert_eq!(out, body);
    }

    #[test]
    fn noop_when_nothing_to_rename() {
        let body = Bytes::from(json!({"candidates": [{}]}).to_string());
        let out = normalize_vertex_response(body.clone());
        assert_eq!(out, body);
    }
}
