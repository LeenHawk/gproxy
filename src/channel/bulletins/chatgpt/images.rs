//! Image generation / edit for the chatgpt web channel.
//!
//! Both flows are channel-driven multi-step exchanges
//! ([`PreparedRequest::custom`](crate::channel::PreparedRequest::custom)): the
//! pipeline injects the resolved client and [`run`] orchestrates
//! `POST /f/conversation` → (poll `conversation/{cid}`) → download → OpenAI
//! `images.response`. Generation forces `system_hints = ["picture_v2"]`; edit
//! first uploads the source image (see [`super::image_upload`]) and attaches it
//! to the user turn. Pointer extraction is ported verbatim from v1
//! `channels/chatgpt/image.rs`.

use std::sync::Arc;

use bytes::Bytes;
use serde_json::{Value, json};

use super::image_download::{download_image_b64, poll_conversation_for_images};
use super::sse::{Event, PatchKind, SseDecoder};
use crate::http::client::{ClientError, UpstreamClient};

/// A pointer to an image stored in ChatGPT's file service. `file-service://`
/// ids are kept as-is; `sediment://` ids are prefixed with `sed:` so the
/// downloader routes to the right endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImagePointer {
    pub id: String,
    pub conversation_id: String,
}

/// Extract image pointers + conversation id from a full SSE-v1 body, walking
/// the same decoder the chat path uses for asset-pointer parts and pointer
/// strings embedded in delta text.
pub(super) fn extract_image_pointers(body: &[u8]) -> (Vec<ImagePointer>, Option<String>) {
    let mut decoder = SseDecoder::new();
    decoder.feed(body);
    decoder.finish();

    let mut conversation_id: Option<String> = None;
    let mut ids: Vec<String> = Vec::new();

    while let Some(event) = decoder.next_event() {
        let Event::Delta(delta) = event else { continue };
        for patch in delta.patches {
            // Whole-wrapper "add": message + conversation_id.
            if patch.op == PatchKind::Add && patch.path.is_empty() {
                if let Some(cid) = patch.value.get("conversation_id").and_then(Value::as_str) {
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
            // Incremental text appends / replaces can carry the pointer string.
            if (patch.op == PatchKind::Append || patch.op == PatchKind::Replace)
                && patch.path == "/message/content/parts/0"
                && let Some(text) = patch.value.as_str()
            {
                scan_text_for_pointers(text, &mut ids);
            }
            // Whole `parts` array replaced with new multimodal parts.
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

pub(super) fn collect_pointers_from_parts(parts: &[Value], ids: &mut Vec<String>) {
    for part in parts {
        if let Some(ptr) = part.get("asset_pointer").and_then(Value::as_str) {
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

/// Build an OpenAI `images.response` body from `(b64, revised_prompt)` items.
fn build_openai_images_response(items: Vec<(String, String)>) -> Bytes {
    let data: Vec<Value> = items
        .into_iter()
        .map(|(b64, revised_prompt)| json!({ "b64_json": b64, "revised_prompt": revised_prompt }))
        .collect();
    let body = json!({ "created": crate::util::time::unix_now(), "data": data });
    Bytes::from(serde_json::to_vec(&body).unwrap_or_default())
}

/// Orchestrate the full image gen/edit exchange. Errors map to
/// [`ClientError::Transport`].
pub(super) async fn run(
    client: Arc<dyn UpstreamClient>,
    secret: Value,
    base: String,
    model: String,
    inbound: Bytes,
    is_edit: bool,
) -> Result<http::Response<Bytes>, ClientError> {
    run_inner(client, secret, base, model, inbound, is_edit)
        .await
        .map_err(ClientError::Transport)
}

async fn run_inner(
    client: Arc<dyn UpstreamClient>,
    secret: Value,
    base: String,
    model: String,
    inbound: Bytes,
    is_edit: bool,
) -> Result<http::Response<Bytes>, String> {
    // 1. Build the chat-shaped body + (edit) upload the source image.
    let (openai_json, upload) = if is_edit {
        let parsed = super::image_upload::parse_edit_body(&inbound)?;
        let up = super::image_upload::upload(&client, &secret, &base, &parsed).await?;
        (
            json!({ "messages": [{ "role": "user", "content": parsed.prompt }] }),
            Some(up),
        )
    } else {
        let req: Value =
            serde_json::from_slice(&inbound).map_err(|e| format!("image request parse: {e}"))?;
        let prompt = req.get("prompt").and_then(Value::as_str).unwrap_or("");
        (
            json!({ "messages": [{ "role": "user", "content": prompt }] }),
            None,
        )
    };

    // 2. /f/conversation body, FORCING picture_v2 (edit attaches the asset).
    //    Image gen MUST use a persistent (non-temporary) conversation: the image
    //    materializes asynchronously and is fetched by polling
    //    `GET /backend-api/conversation/{cid}`, which 404s for temporary chats
    //    (they are never persisted). So we override temporary_chat → false here,
    //    even though plain chat defaults to temporary.
    let mut body_map =
        super::request_builder::build_conversation_body(&openai_json, &model, false, None);
    body_map.insert("system_hints".to_string(), json!(["picture_v2"]));
    if let Some(up) = upload.as_ref() {
        super::image_upload::attach_uploaded_image(&mut body_map, up);
    }
    let body = serde_json::to_vec(&Value::Object(body_map))
        .map_err(|e| format!("image body serialize: {e}"))?;

    // 3. POST the conversation; buffer the response.
    let url = format!("{base}/backend-api/f/conversation");
    let mut req = http::Request::post(url)
        .body(Bytes::from(body))
        .map_err(|e| format!("image conversation build: {e}"))?;
    super::auth::apply_request_headers(&mut req, &secret).map_err(|e| e.to_string())?;
    let resp = client
        .send(req)
        .await
        .map_err(|e| format!("image conversation send: {e}"))?;
    let resp_body = resp.into_body();

    // 4. Pointers: immediate, else poll the conversation (≤180s).
    let (immediate, conv_id) = extract_image_pointers(&resp_body);
    let conv_id = conv_id.ok_or("image response missing conversation_id")?;
    let pointers = if immediate.is_empty() {
        poll_conversation_for_images(&client, &secret, &base, &conv_id, 180).await?
    } else {
        immediate
    };

    // 5. Download each pointer to b64 (empty revised_prompt, like v1).
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for ptr in &pointers {
        match download_image_b64(&client, &secret, &base, ptr).await {
            Ok(b64) => items.push((b64, String::new())),
            Err(e) => {
                tracing::warn!(error = %e, pointer = %ptr.id, "chatgpt image download failed");
                errors.push(format!("{}: {e}", ptr.id));
            }
        }
    }
    if items.is_empty() {
        return Err(format!(
            "no images downloaded ({} pointer(s); errors: {})",
            pointers.len(),
            if errors.is_empty() {
                "none".into()
            } else {
                errors.join(" | ")
            }
        ));
    }

    // 6. Wrap in OpenAI images.response.
    http::Response::builder()
        .status(200)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(build_openai_images_response(items))
        .map_err(|e| format!("image response build: {e}"))
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
