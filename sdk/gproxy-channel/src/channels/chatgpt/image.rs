//! Image generation support for the ChatGPT web channel.
//!
//! The upstream always serves a single conversation SSE stream. When the
//! user prompt asks for an image, assistant deltas include
//! `file-service://<id>` / `sediment://<id>` pointers in the message
//! payload. We scan those out and download the actual bytes so we can
//! return a standard OpenAI `images.response` body.

use serde_json::{Value, json};

use super::sse_v1::{Event, PatchKind, SseDecoder};

/// A pointer to an image stored in ChatGPT's file service, extracted from
/// an SSE stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePointer {
    /// Raw id portion (after the scheme prefix). `file-service://` ids are
    /// stored as-is; `sediment://` ids are prefixed with `sed:` so callers
    /// can route to the right download endpoint.
    pub id: String,
    pub conversation_id: String,
}

/// Extract the image pointers + conversation id from a full SSE v1 body.
///
/// Uses the same decoder the chat path uses, walking delta events for
/// paths that look like image assets.
pub fn extract_image_pointers(body: &[u8]) -> (Vec<ImagePointer>, Option<String>) {
    let mut decoder = SseDecoder::new();
    decoder.feed(body);

    let mut conversation_id: Option<String> = None;
    let mut ids: Vec<String> = Vec::new();

    while let Some(event) = decoder.next_event() {
        let Event::Delta(delta) = event else { continue };

        for patch in delta.patches {
            // Whole-wrapper "add" events contain message + conversation_id.
            if patch.op == PatchKind::Add && patch.path.is_empty() {
                if let Some(cid) = patch.value.get("conversation_id").and_then(|v| v.as_str()) {
                    conversation_id = Some(cid.to_string());
                }
                if let Some(parts) = patch
                    .value
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array())
                {
                    collect_pointers_from_parts(parts, &mut ids);
                }
                continue;
            }

            // Incremental text appends can contain the pointer string.
            if patch.op == PatchKind::Append
                && patch.path == "/message/content/parts/0"
                && let Some(text) = patch.value.as_str()
            {
                scan_text_for_pointers(text, &mut ids);
            }

            // Replace on parts/0 (rare but seen in some model paths).
            if patch.op == PatchKind::Replace
                && patch.path == "/message/content/parts/0"
                && let Some(text) = patch.value.as_str()
            {
                scan_text_for_pointers(text, &mut ids);
            }

            // `/message/content/parts` replaced wholesale with a new array
            // of multimodal parts (some image paths).
            if patch.op == PatchKind::Replace
                && patch.path == "/message/content/parts"
                && let Some(parts) = patch.value.as_array()
            {
                collect_pointers_from_parts(parts, &mut ids);
            }
        }
    }

    ids.sort();
    ids.dedup();
    let cid = conversation_id.clone().unwrap_or_default();
    (
        ids.into_iter()
            .map(|id| ImagePointer {
                id,
                conversation_id: cid.clone(),
            })
            .collect(),
        conversation_id,
    )
}

fn collect_pointers_from_parts(parts: &[Value], ids: &mut Vec<String>) {
    for part in parts {
        if let Some(ptr) = part.get("asset_pointer").and_then(|v| v.as_str()) {
            push_pointer(ptr, ids);
        }
    }
}

fn scan_text_for_pointers(text: &str, ids: &mut Vec<String>) {
    for scheme in ["file-service://", "sediment://"] {
        let mut rest = text;
        while let Some(idx) = rest.find(scheme) {
            let start = idx + scheme.len();
            let tail = &rest[start..];
            let end = tail
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(tail.len());
            if end > 0 {
                let id = &tail[..end];
                let stored = if scheme.starts_with("sed") {
                    format!("sed:{id}")
                } else {
                    id.to_string()
                };
                push_pointer_raw(&stored, ids);
            }
            rest = &tail[end..];
        }
    }
}

fn push_pointer(raw: &str, ids: &mut Vec<String>) {
    if let Some(id) = raw.strip_prefix("file-service://") {
        push_pointer_raw(id, ids);
    } else if let Some(id) = raw.strip_prefix("sediment://") {
        push_pointer_raw(&format!("sed:{id}"), ids);
    }
}

fn push_pointer_raw(id: &str, ids: &mut Vec<String>) {
    if !id.is_empty() && !ids.iter().any(|x| x == id) {
        ids.push(id.to_string());
    }
}

/// Async: given a file pointer + authenticated client, resolve to a
/// base64-encoded image body.
pub async fn download_image_b64(
    client: &wreq::Client,
    access_token: &str,
    device_id: &str,
    ptr: &ImagePointer,
) -> Result<String, String> {
    let (endpoint_id, is_sediment) = if let Some(rest) = ptr.id.strip_prefix("sed:") {
        (rest.to_string(), true)
    } else {
        (ptr.id.clone(), false)
    };

    let download_url_body = if is_sediment {
        format!(
            "https://chatgpt.com/backend-api/conversation/{}/attachment/{}/download",
            ptr.conversation_id, endpoint_id
        )
    } else {
        format!(
            "https://chatgpt.com/backend-api/files/download/{}?conversation_id={}&inline=false",
            endpoint_id, ptr.conversation_id
        )
    };

    let headers = super::session::standard_headers(access_token);
    let step1_resp = client
        .get(&download_url_body)
        .header("oai-device-id", device_id)
        .headers(headers.into())
        .send()
        .await
        .map_err(|e| format!("download step1: {e}"))?;
    let step1_status = step1_resp.status();
    let step1_bytes = step1_resp
        .bytes()
        .await
        .map_err(|e| format!("download step1 read: {e}"))?;
    if !step1_status.is_success() {
        return Err(format!(
            "download step1 {step1_status}: {}",
            String::from_utf8_lossy(&step1_bytes)
                .chars()
                .take(200)
                .collect::<String>()
        ));
    }
    let step1: Value =
        serde_json::from_slice(&step1_bytes).map_err(|e| format!("download step1 decode: {e}"))?;

    let download_url = step1
        .get("download_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            format!(
                "missing download_url in step1 response: {}",
                String::from_utf8_lossy(&step1_bytes)
                    .chars()
                    .take(200)
                    .collect::<String>()
            )
        })?;
    tracing::debug!(download_url = %download_url, "chatgpt image download url");

    let step2_resp = client
        .get(download_url)
        // Browser fetch of the estuary URL uses pre-signed sig= in the
        // querystring and no Authorization header; sending Bearer here
        // causes the server to 403 with "File stream access denied".
        .header(
            "accept",
            "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8",
        )
        .header("referer", "https://chatgpt.com/")
        .send()
        .await
        .map_err(|e| format!("download step2: {e}"))?;
    let step2_status = step2_resp.status();
    let bytes = step2_resp
        .bytes()
        .await
        .map_err(|e| format!("download step2 read: {e}"))?;
    if !step2_status.is_success() {
        return Err(format!(
            "download step2 {step2_status}: {}",
            String::from_utf8_lossy(&bytes)
                .chars()
                .take(200)
                .collect::<String>()
        ));
    }

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    Ok(STANDARD.encode(&bytes))
}

/// Image generation is asynchronous on chatgpt.com: the initial SSE merely
/// acknowledges the job (with a "Processing image" tool message); the
/// actual file-service pointers appear later in `conversation/{cid}`.
/// This helper polls that endpoint until the pointers show up or a
/// deadline expires.
pub async fn poll_conversation_for_images(
    client: &wreq::Client,
    access_token: &str,
    device_id: &str,
    conversation_id: &str,
    deadline_secs: u64,
) -> Result<Vec<ImagePointer>, String> {
    let url = format!(
        "https://chatgpt.com/backend-api/conversation/{}",
        conversation_id
    );
    let started = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(deadline_secs);
    let headers = super::session::standard_headers(access_token);

    loop {
        let resp = client
            .get(&url)
            .header("oai-device-id", device_id)
            .headers(headers.clone().into())
            .send()
            .await
            .map_err(|e| format!("poll conv: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if started.elapsed() >= timeout {
                return Err(format!("poll conv timed out with status {status}"));
            }
            continue;
        }
        let json: Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                if started.elapsed() >= timeout {
                    return Err("poll conv: bad body".into());
                }
                continue;
            }
        };
        let mut ids = Vec::new();
        if let Some(mapping) = json.get("mapping").and_then(|m| m.as_object()) {
            for node in mapping.values() {
                let message = node.get("message");
                let Some(msg) = message else { continue };
                let role = msg
                    .get("author")
                    .and_then(|a| a.get("role"))
                    .and_then(|v| v.as_str());
                if role != Some("tool") {
                    continue;
                }
                let async_kind = msg
                    .get("metadata")
                    .and_then(|m| m.get("async_task_type"))
                    .and_then(|v| v.as_str());
                if async_kind != Some("image_gen") {
                    continue;
                }
                let content_type = msg
                    .get("content")
                    .and_then(|c| c.get("content_type"))
                    .and_then(|v| v.as_str());
                if content_type != Some("multimodal_text") {
                    continue;
                }
                if let Some(parts) = msg
                    .get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array())
                {
                    collect_pointers_from_parts(parts, &mut ids);
                }
            }
        }
        if !ids.is_empty() {
            return Ok(ids
                .into_iter()
                .map(|id| ImagePointer {
                    id,
                    conversation_id: conversation_id.to_string(),
                })
                .collect());
        }
        if started.elapsed() >= timeout {
            return Err("poll conv: no image pointers before deadline".into());
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

/// Build an OpenAI `images.response` wrapper around the given b64 images.
pub fn build_openai_images_response(items: Vec<(String, String)>) -> Value {
    let data: Vec<Value> = items
        .into_iter()
        .map(|(b64, revised_prompt)| {
            json!({
                "b64_json": b64,
                "revised_prompt": revised_prompt,
            })
        })
        .collect();
    json!({
        "created": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        "data": data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_pointers_from_asset_pointer_part() {
        let body = br#"event: delta_encoding
data: "v1"

event: delta
data: {"p":"","o":"add","v":{"message":{"id":"m1","author":{"role":"assistant"},"content":{"content_type":"multimodal_text","parts":[{"asset_pointer":"file-service://file_abc123","size_bytes":100,"width":512,"height":512}]},"status":"finished_successfully"},"conversation_id":"conv-1"},"c":0}

"#;
        let (ptrs, cid) = extract_image_pointers(body);
        assert_eq!(cid.as_deref(), Some("conv-1"));
        assert_eq!(ptrs.len(), 1);
        assert_eq!(ptrs[0].id, "file_abc123");
    }

    #[test]
    fn extracts_pointers_from_text_mentions() {
        let body = br#"event: delta
data: {"v":[{"p":"/message/content/parts/0","o":"append","v":"Here is your image: file-service://file_xyz789"}]}

"#;
        let (ptrs, _) = extract_image_pointers(body);
        assert!(ptrs.iter().any(|p| p.id == "file_xyz789"));
    }

    #[test]
    fn handles_sediment_scheme() {
        let body = br#"event: delta
data: {"p":"","o":"add","v":{"message":{"content":{"content_type":"multimodal_text","parts":[{"asset_pointer":"sediment://sedfoo_bar"}]},"author":{"role":"assistant"}},"conversation_id":"c"},"c":0}

"#;
        let (ptrs, _) = extract_image_pointers(body);
        assert_eq!(ptrs.len(), 1);
        assert_eq!(ptrs[0].id, "sed:sedfoo_bar");
    }
}
