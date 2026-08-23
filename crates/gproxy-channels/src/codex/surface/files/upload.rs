use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{
    ChannelError, CredentialId, SurfaceBody, SurfaceReply, SurfaceServices, SynthCtx,
};
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::super::helpers::{
    FILE_KIND, empty_object, invoke, json_reply, reply_json, request, save_binding,
    transport_reply, unix_now,
};
use super::multipart;

const MAX_FILE_BYTES: usize = 512 * 1024 * 1024;
const FINALIZE_ATTEMPTS: usize = 120;

pub(super) async fn hosted_create(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let metadata = serde_json::from_slice::<Value>(ctx.body)
        .map_err(|error| ChannelError::Prepare(format!("file metadata JSON: {error}")))?;
    let file_size = metadata
        .get("file_size")
        .and_then(Value::as_u64)
        .ok_or_else(|| ChannelError::Prepare("file metadata missing file_size".into()))?;
    let filename = metadata
        .get("file_name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Prepare("file metadata missing file_name".into()))?;
    let purpose = metadata
        .get("use_case")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Prepare("file metadata missing use_case".into()))?;
    if file_size > MAX_FILE_BYTES as u64 {
        return Ok(json_reply(
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({"error":"file too large"}),
        ));
    }
    let reply = create_raw(services, ctx.body.clone(), services.credential).await?;
    if reply.status.is_success() {
        let hosted = reply_json(&reply)?;
        let file_id = hosted
            .get("file_id")
            .and_then(Value::as_str)
            .ok_or_else(|| ChannelError::Decode("hosted file response missing file_id".into()))?
            .to_owned();
        let file = file_object(&file_id, filename, file_size, purpose, "uploaded");
        save_file(services, &file_id, services.credential, file, hosted).await?;
    }
    Ok(reply)
}

pub(super) async fn openai_create(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let upload = multipart::parse(ctx.headers, ctx.body)?;
    if upload.file.len() > MAX_FILE_BYTES {
        return Ok(json_reply(
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({"error":"file too large"}),
        ));
    }
    let metadata = json!({
        "file_name":upload.filename,
        "file_size":upload.file.len(),
        "use_case":"codex"
    });
    let created = create_raw(
        services,
        Bytes::from(serde_json::to_vec(&metadata).expect("metadata serializes")),
        services.credential,
    )
    .await?;
    if !created.status.is_success() {
        return Ok(created);
    }
    let hosted = reply_json(&created)?;
    let upload_url = hosted
        .get("upload_url")
        .and_then(Value::as_str)
        .ok_or_else(|| ChannelError::Decode("hosted file response missing upload_url".into()))?
        .to_owned();
    let file_id = hosted
        .get("file_id")
        .and_then(Value::as_str)
        .ok_or_else(|| ChannelError::Decode("hosted file response missing file_id".into()))?
        .to_owned();
    let pending = file_object(
        &file_id,
        &upload.filename,
        upload.file.len() as u64,
        &upload.purpose,
        "uploaded",
    );
    save_file(services, &file_id, services.credential, pending, hosted).await?;
    let put = http::Request::put(upload_url)
        .header("x-ms-blob-type", "BlockBlob")
        .header(http::header::CONTENT_TYPE, upload.mime_type)
        .body(upload.file)
        .map_err(transport_reply)?;
    let uploaded = services
        .invoke
        .ok_or_else(|| ChannelError::Prepare("surface has no upstream capability".into()))?
        .fetch_presigned(put)
        .await
        .map_err(transport_reply)?;
    if !uploaded.status.is_success() {
        return Ok(uploaded);
    }
    if let SurfaceBody::Stream(mut body) = uploaded.body {
        while let Some(chunk) = body.next().await {
            chunk.map_err(transport_reply)?;
        }
    }
    let finalized = finalize(services, &file_id, services.credential).await?;
    let file = file_object(
        &file_id,
        &upload.filename,
        metadata["file_size"].as_u64().unwrap_or_default(),
        &upload.purpose,
        "processed",
    );
    save_file(
        services,
        &file_id,
        services.credential,
        file.clone(),
        finalized,
    )
    .await?;
    Ok(json_reply(StatusCode::OK, file))
}

pub(super) async fn content(
    services: &SurfaceServices<'_>,
    file_id: &str,
    credential: CredentialId,
) -> Result<SurfaceReply, ChannelError> {
    let value = finalize(services, file_id, credential).await?;
    let url = value
        .get("download_url")
        .and_then(Value::as_str)
        .ok_or_else(|| ChannelError::Decode("file download URL missing".into()))?;
    let request = http::Request::get(url)
        .body(Bytes::new())
        .map_err(transport_reply)?;
    services
        .invoke
        .ok_or_else(|| ChannelError::Prepare("surface has no upstream capability".into()))?
        .fetch_presigned(request)
        .await
        .map_err(transport_reply)
}

pub(super) async fn finalize(
    services: &SurfaceServices<'_>,
    file_id: &str,
    credential: CredentialId,
) -> Result<Value, ChannelError> {
    for attempt in 0..FINALIZE_ATTEMPTS {
        let reply = invoke(
            services,
            request(
                "hosted_file_finalize",
                Method::POST,
                format!("/files/{file_id}/uploaded"),
                None,
                &Default::default(),
                empty_object(),
                Some(credential),
            ),
        )
        .await?;
        if !reply.status.is_success() {
            return Err(ChannelError::Prepare(format!(
                "hosted file finalize returned {}",
                reply.status
            )));
        }
        let value = reply_json(&reply)?;
        if value.get("status").and_then(Value::as_str) != Some("retry") {
            return Ok(value);
        }
        if attempt + 1 == FINALIZE_ATTEMPTS {
            break;
        }
        services
            .invoke
            .expect("finalize requires upstream capability")
            .wait(Duration::from_millis(250))
            .await;
    }
    Err(ChannelError::Prepare(
        "hosted file upload not ready after bounded polling".into(),
    ))
}

async fn create_raw(
    services: &SurfaceServices<'_>,
    body: Bytes,
    credential: CredentialId,
) -> Result<SurfaceReply, ChannelError> {
    invoke(
        services,
        request(
            "hosted_file_create",
            Method::POST,
            "/files".into(),
            None,
            &Default::default(),
            body,
            Some(credential),
        ),
    )
    .await
}

async fn save_file(
    services: &SurfaceServices<'_>,
    file_id: &str,
    credential: CredentialId,
    file: Value,
    hosted: Value,
) -> Result<(), ChannelError> {
    save_binding(
        services,
        FILE_KIND,
        file_id,
        credential,
        json!({"file":file,"hosted":hosted}),
    )
    .await
}

fn file_object(id: &str, filename: &str, bytes: u64, purpose: &str, status: &str) -> Value {
    json!({
        "id":id,"object":"file","bytes":bytes,"created_at":unix_now(),
        "filename":filename,"purpose":purpose,"status":status,"status_details":null
    })
}
