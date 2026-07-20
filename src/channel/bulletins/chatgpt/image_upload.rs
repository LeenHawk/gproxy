//! Input parsing + 3-step file upload for `/v1/images/edits`.
//!
//! Clients send image-edit requests as **multipart/form-data** (`image` +
//! `prompt` parts; OpenAI SDK default) or as **JSON** (`{image, prompt}` where
//! `image` is a `data:<mime>;base64,…` URL). Both flatten into [`ParsedEdit`],
//! whose bytes are uploaded to chatgpt.com via the 3-step files API before the
//! `/f/conversation` body references them via an `image_asset_pointer`.
//!
//! Ported from v1 `channels/chatgpt/image_edit.rs`, adapted from `wreq::Client`
//! to the v2 [`UpstreamClient`].

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use super::headers::standard_headers;
use crate::http::client::UpstreamClient;

mod input;

use input::probe_dimensions;
pub(super) use input::{ParsedEdit, parse_edit_body};

/// Server-assigned file id + image dimensions, needed for the conversation
/// body's `image_asset_pointer`.
#[derive(Debug, Clone)]
pub(super) struct UploadResult {
    pub file_id: String,
    pub size_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub filename: String,
    pub mime_type: String,
}

/// Three-step raw-image upload to chatgpt.com's files API:
/// 1. `POST /backend-api/files` → `{upload_url, file_id}` (presigned Azure Blob).
/// 2. `PUT <upload_url>` raw bytes with `x-ms-blob-type: BlockBlob`.
/// 3. `POST /backend-api/files/process_upload_stream` to activate the file.
pub(super) async fn upload(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
    base: &str,
    parsed: &ParsedEdit,
) -> Result<UploadResult, String> {
    let token = secret
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("missing access_token")?;
    let (width, height) = probe_dimensions(&parsed.image_bytes).unwrap_or((1024, 1024));
    let size_bytes = parsed.image_bytes.len() as u64;

    // Step 1: request the upload URL.
    let step1_body = serde_json::json!({
        "file_name": parsed.filename,
        "file_size": size_bytes,
        "use_case": "multimodal",
        "timezone_offset_min": -480,
        "reset_rate_limits": false,
        "store_in_library": true,
        "library_persistence_mode": "opportunistic",
    });
    let step1 = post_json(
        client,
        token,
        &format!("{base}/backend-api/files"),
        &step1_body,
    )
    .await?;
    if !step1.status().is_success() {
        return Err(format!(
            "upload step1 {}: {}",
            step1.status(),
            snip(step1.body())
        ));
    }
    let s1: Value =
        serde_json::from_slice(step1.body()).map_err(|e| format!("upload step1 decode: {e}"))?;
    let upload_url = s1
        .get("upload_url")
        .and_then(Value::as_str)
        .ok_or("upload step1: missing upload_url")?
        .to_string();
    let file_id = s1
        .get("file_id")
        .and_then(Value::as_str)
        .ok_or("upload step1: missing file_id")?
        .to_string();

    // Step 2: PUT raw bytes to Azure Blob.
    let mut put_req = http::Request::put(&upload_url)
        .body(Bytes::from(parsed.image_bytes.clone()))
        .map_err(|e| format!("upload step2 build: {e}"))?;
    let h = put_req.headers_mut();
    if let Ok(v) = http::HeaderValue::from_str(&parsed.mime_type) {
        h.insert(http::header::CONTENT_TYPE, v);
    }
    h.insert(
        http::HeaderName::from_static("x-ms-blob-type"),
        http::HeaderValue::from_static("BlockBlob"),
    );
    let step2 = client
        .send(put_req)
        .await
        .map_err(|e| format!("upload step2: {e}"))?;
    if !step2.status().is_success() {
        return Err(format!(
            "upload step2 {}: {}",
            step2.status(),
            snip(step2.body())
        ));
    }

    // Step 3: activate.
    let step3_body = serde_json::json!({
        "file_id": file_id,
        "use_case": "multimodal",
        "index_for_retrieval": false,
        "file_name": parsed.filename,
        "library_persistence_mode": "opportunistic",
        "metadata": {"store_in_library": true},
    });
    let step3 = post_json(
        client,
        token,
        &format!("{base}/backend-api/files/process_upload_stream"),
        &step3_body,
    )
    .await?;
    if !step3.status().is_success() {
        return Err(format!(
            "upload step3 {}: {}",
            step3.status(),
            snip(step3.body())
        ));
    }

    Ok(UploadResult {
        file_id,
        size_bytes,
        width,
        height,
        filename: parsed.filename.clone(),
        mime_type: parsed.mime_type.clone(),
    })
}

/// POST a JSON body with the standard chatgpt-web headers.
async fn post_json(
    client: &Arc<dyn UpstreamClient>,
    token: &str,
    url: &str,
    body: &Value,
) -> Result<http::Response<Bytes>, String> {
    let bytes = serde_json::to_vec(body).map_err(|e| format!("encode: {e}"))?;
    let mut req = http::Request::post(url)
        .body(Bytes::from(bytes))
        .map_err(|e| format!("build: {e}"))?;
    *req.headers_mut() = standard_headers(token);
    client.send(req).await.map_err(|e| format!("send: {e}"))
}

fn snip(body: &[u8]) -> String {
    String::from_utf8_lossy(body).chars().take(200).collect()
}

/// Attach an uploaded image onto the conversation body's single user message:
/// the content becomes `multimodal_text`, `parts[0]` an `image_asset_pointer`,
/// the prompt text `parts[1]`, and `metadata.attachments[0]` describes the file.
pub(super) fn attach_uploaded_image(
    body: &mut serde_json::Map<String, Value>,
    upload: &UploadResult,
) {
    let Some(messages) = body.get_mut("messages").and_then(|m| m.as_array_mut()) else {
        return;
    };
    if messages.is_empty() {
        return;
    }
    let user_msg = &mut messages[0];
    let prompt_text = user_msg
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let asset = serde_json::json!({
        "content_type": "image_asset_pointer",
        "asset_pointer": format!("sediment://{}", upload.file_id),
        "size_bytes": upload.size_bytes,
        "width": upload.width,
        "height": upload.height,
    });
    if let Some(obj) = user_msg.get_mut("content").and_then(Value::as_object_mut) {
        obj.insert(
            "content_type".into(),
            Value::String("multimodal_text".into()),
        );
        let mut parts = vec![asset];
        if !prompt_text.is_empty() {
            parts.push(Value::String(prompt_text));
        }
        obj.insert("parts".into(), Value::Array(parts));
    }
    if let Some(md) = user_msg.get_mut("metadata").and_then(Value::as_object_mut) {
        md.insert(
            "attachments".into(),
            Value::Array(vec![serde_json::json!({
                "id": upload.file_id,
                "size": upload.size_bytes,
                "name": upload.filename,
                "mime_type": upload.mime_type,
                "width": upload.width,
                "height": upload.height,
                "source": "library",
                "is_big_paste": false,
            })]),
        );
    }
}
