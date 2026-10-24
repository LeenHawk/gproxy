//! Desktop attachment flow: features.js:32684, 38122 and 39163. Base64 inputs
//! are normalized before upload. Credential-scoped content hashes reuse
//! uploads for the desktop's two-minute preview lifetime, including history.

use super::{
    auth,
    config::{OrcaTermConfig, invalid, new_id, now_ms},
    image_processing,
    request::{bad, configured_headers},
};
use crate::channels::shared::compatible::http::read_body;
use crate::{
    OutboundClient,
    channel::{ChannelError, ChannelState, CredentialView, ProviderView},
};
use base64::Engine as _;
use gproxy_protocol::{HttpBody, capability::StateWrite};
use http::{HeaderValue, Method, header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime};

const MESSAGE_BYTES: usize = 10 * 1024 * 1024;
const REQUEST_BYTES: usize = 50 * 1024 * 1024;
const MESSAGE_IMAGES: usize = 5;

pub(super) struct Upload {
    target: String,
    bytes: Vec<u8>,
    mime: &'static str,
    extension: &'static str,
}

pub(super) fn content(
    value: &Value,
    target: &str,
    uploads: &mut Vec<Upload>,
) -> Result<(String, Vec<Value>), ChannelError> {
    if let Some(text) = value.as_str() {
        return Ok((text.into(), vec![]));
    }
    let parts = value
        .as_array()
        .ok_or_else(|| bad("message content must be text or content parts"))?;
    let mut text = String::new();
    let mut files = Vec::new();
    let mut message_bytes = 0;
    let mut request_bytes = uploads
        .iter()
        .map(|upload| upload.bytes.len())
        .sum::<usize>();
    for part in parts {
        match part["type"].as_str() {
            Some("text") => text.push_str(
                part["text"]
                    .as_str()
                    .ok_or_else(|| bad("text part has no text"))?,
            ),
            Some("image_url") => {
                if files.len() >= MESSAGE_IMAGES {
                    return Err(bad("at most five images are supported per message"));
                }
                let url = part
                    .pointer("/image_url/url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| bad("image_url.url must be a base64 data URL"))?;
                let (prefix, data) = url
                    .split_once(',')
                    .ok_or_else(|| bad("images require base64 data URLs"))?;
                let mime = prefix
                    .strip_prefix("data:")
                    .and_then(|prefix| prefix.strip_suffix(";base64"))
                    .ok_or_else(|| bad("images require base64 data URLs"))?;
                let mime = match mime {
                    "image/png" => "image/png",
                    "image/jpeg" => "image/jpeg",
                    "image/webp" => "image/webp",
                    "image/gif" => "image/gif",
                    _ => return Err(bad("supported image types are PNG, JPEG, WebP and GIF")),
                };
                if data.len() > REQUEST_BYTES.div_ceil(3) * 4 {
                    return Err(bad("image input exceeds 50 MiB"));
                }
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|_| bad("invalid base64 image"))?;
                let (bytes, mime, extension) = image_processing::prepare(bytes, mime)?;
                message_bytes += bytes.len();
                request_bytes += bytes.len();
                if bytes.is_empty()
                    || message_bytes > MESSAGE_BYTES
                    || request_bytes > REQUEST_BYTES
                {
                    return Err(bad(
                        "images must be nonempty and fit 10 MiB per message / 50 MiB per request",
                    ));
                }
                uploads.push(Upload {
                    target: format!("{target}/{}", files.len()),
                    bytes,
                    mime,
                    extension,
                });
                files.push(Value::Null);
            }
            _ => return Err(bad("only text and image content parts are supported")),
        }
    }
    Ok((text, files))
}

pub(super) async fn upload_all(
    body: &mut Value,
    uploads: Vec<Upload>,
    provider: ProviderView<'_>,
    credential: CredentialView<'_>,
    client: &dyn OutboundClient,
    state: &dyn ChannelState,
) -> Result<(), ChannelError> {
    let config = OrcaTermConfig::from_view(provider)?;
    for upload in uploads {
        let mut hash = Sha256::new();
        for part in [
            config.base_url(provider).as_bytes(),
            credential.id.as_bytes(),
            auth::fact(credential, "user_id")
                .unwrap_or_default()
                .as_bytes(),
            upload.mime.as_bytes(),
            upload.bytes.as_slice(),
        ] {
            hash.update((part.len() as u64).to_le_bytes());
            hash.update(part);
        }
        let digest: String = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let key = format!("orcaterm:image:v1:{digest}");
        let cached = state.get(&key).await?;
        if let Some(entry) = &cached {
            let file: Value = serde_json::from_slice(&entry.payload)
                .map_err(|_| invalid("invalid cached image upload"))?;
            *body.pointer_mut(&upload.target).expect("attachment slot") = file;
            continue;
        }
        let id = new_id()?;
        let filename = format!("{digest}.{}", upload.extension);
        let path = format!("ai-uploads/{filename}");
        let boundary = format!("----gproxy{id}");
        let mut data = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"filePath\"\r\n\r\n{path}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {}\r\n\r\n", upload.mime).into_bytes();
        data.extend_from_slice(&upload.bytes);
        drop(upload.bytes);
        data.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let mut request = http::Request::builder()
            .method(Method::POST)
            .uri(format!("{}/drive/llmSave", config.base_url(provider)))
            .body(HttpBody::Bytes(data.into()))
            .map_err(|e| bad(&e.to_string()))?;
        request.headers_mut().extend(configured_headers(&config)?);
        request
            .headers_mut()
            .extend(auth::headers(credential.secret)?);
        request.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
                .map_err(|e| bad(&e.to_string()))?,
        );
        request
            .headers_mut()
            .insert(header::ACCEPT, HeaderValue::from_static("application/json"));
        let response = client.send(request).await?;
        let bytes = read_body(response.body).await?;
        if !response.status.is_success() {
            return Err(ChannelError::UpstreamResponse {
                status: response.status,
                body: bytes,
            });
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| invalid("invalid image upload JSON"))?;
        let data = &value["data"];
        if data["uploaded"] == false || data["passed"] == false {
            return Err(invalid("image upload was rejected"));
        }
        let preview = data["llmPreviewUrl"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("image upload has no llmPreviewUrl"))?;
        let mut uri = url::Url::parse(preview).map_err(|_| invalid("invalid image preview URL"))?;
        if !matches!(uri.scheme(), "http" | "https") {
            return Err(invalid("image preview URL must use HTTP(S)"));
        }
        let path = data["filePath"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(&path);
        let pairs: Vec<_> = uri
            .query_pairs()
            .filter(|(key, _)| key != "filePath")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        uri.set_fragment(None);
        uri.query_pairs_mut()
            .clear()
            .extend_pairs(pairs)
            .append_pair("filePath", path);
        let uri = uri.to_string();
        let metadata = json!({"kind":"lightai_drive_image","filePath":path,"llmPreviewUrl":preview,"uploadedAt":now_ms()});
        let file = json!({"name":filename,"mimeType":upload.mime,"uri":uri,"details":{
            "acp":{"type":"image","uri":uri,"mimeType":upload.mime,"_meta":{"orcaterm":metadata}},
            "orcaterm":metadata
        }});
        let _ = state
            .compare_exchange(
                &key,
                cached.map(|entry| entry.version),
                Some(StateWrite {
                    payload: file.to_string().into(),
                    expires_at: Some(
                        SystemTime::UNIX_EPOCH
                            + Duration::from_millis(now_ms().max(0) as u64)
                            + Duration::from_secs(120),
                    ),
                }),
            )
            .await;
        *body
            .pointer_mut(&upload.target)
            .expect("attachment slot created by request builder") = file;
    }
    Ok(())
}
