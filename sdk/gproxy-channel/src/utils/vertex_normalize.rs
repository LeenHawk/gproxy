/// Normalize Vertex/VertexExpress responses to match AI Studio (standard Gemini) format.
///
/// - `citationMetadata.citations` → `citationMetadata.citationSources`
/// - `BLOCKED_REASON_UNSPECIFIED` → `BLOCK_REASON_UNSPECIFIED`
pub fn normalize_vertex_response(body: Vec<u8>) -> Vec<u8> {
    let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };

    let mut changed = false;

    if let Some(candidates) = json
        .get_mut("candidates")
        .and_then(serde_json::Value::as_array_mut)
    {
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
        *br = serde_json::Value::String("BLOCK_REASON_UNSPECIFIED".to_string());
        changed = true;
    }

    if changed {
        serde_json::to_vec(&json).unwrap_or(body)
    } else {
        body
    }
}
