use gproxy_channel_api::{ChannelError, ResourceCtx, ResourceMutation, UsageCtx};
use gproxy_protocol::Operation;
use gproxy_protocol::gemini;
use std::collections::BTreeSet;

pub(super) fn settlement_ready(ctx: UsageCtx<'_>) -> Result<bool, ChannelError> {
    let operation: gemini::VeoOperation =
        serde_json::from_slice(ctx.response_body).map_err(json_error)?;
    Ok(operation.done == Some(true) && operation.error.is_none())
}

pub(super) fn mutations(ctx: ResourceCtx<'_>) -> Result<Vec<ResourceMutation>, ChannelError> {
    match ctx.key.operation() {
        Operation::CreateFile => upload(ctx.response_body),
        Operation::ListFiles => list_files(ctx.response_body),
        Operation::RetrieveFile => file(ctx.response_body, ctx.request_resource),
        Operation::DeleteFile => delete_file(ctx.response_body, ctx.request_resource),
        Operation::CreateVideo | Operation::RetrieveVideo => video(ctx),
        _ => Ok(Vec::new()),
    }
}

fn upload(body: &[u8]) -> Result<Vec<ResourceMutation>, ChannelError> {
    if body.is_empty() {
        return Ok(Vec::new());
    }
    let response: gemini::UploadFileResponse = serde_json::from_slice(body).map_err(json_error)?;
    match response.file {
        Some(file) => Ok(vec![save_file(file)?]),
        None => Err(observe("upload response has no file")),
    }
}

fn list_files(body: &[u8]) -> Result<Vec<ResourceMutation>, ChannelError> {
    let response: gemini::ListFilesResponse = serde_json::from_slice(body).map_err(json_error)?;
    response.files.into_iter().map(save_file).collect()
}

fn file(
    body: &[u8],
    request: Option<(&'static str, &str)>,
) -> Result<Vec<ResourceMutation>, ChannelError> {
    let file: gemini::File = serde_json::from_slice(body).map_err(json_error)?;
    let mutation = save_file(file)?;
    check_mutation_id(request, &mutation)?;
    Ok(vec![mutation])
}

fn delete_file(
    body: &[u8],
    request: Option<(&'static str, &str)>,
) -> Result<Vec<ResourceMutation>, ChannelError> {
    let _: gemini::DeleteFileResponse = serde_json::from_slice(body).map_err(json_error)?;
    let (_, id) = request.ok_or_else(|| observe("delete request has no file id"))?;
    Ok(vec![ResourceMutation::Delete {
        kind: "file",
        id: id.to_owned(),
    }])
}

fn video(ctx: ResourceCtx<'_>) -> Result<Vec<ResourceMutation>, ChannelError> {
    let operation: gemini::VeoOperation =
        serde_json::from_slice(ctx.response_body).map_err(json_error)?;
    let name = operation
        .name
        .as_deref()
        .ok_or_else(|| observe("Veo operation has no name"))?;
    let id = resource_tail(name, "operations")?;
    if let Some((_, request_id)) = ctx.request_resource
        && request_id != id
    {
        return Err(observe("Veo response operation id differs from request"));
    }
    let summary = serde_json::to_value(&operation).map_err(json_error)?;
    let mut mutations = vec![ResourceMutation::Save {
        kind: "video",
        id: id.to_owned(),
        summary,
    }];
    let mut files = BTreeSet::new();
    if let Some(response) = operation.response.as_ref() {
        for pointer in [
            "/generateVideoResponse/generatedSamples",
            "/generateVideoResponse/generatedVideos",
            "/generatedVideos",
        ] {
            let Some(samples) = response
                .pointer(pointer)
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            for sample in samples {
                let Some(uri) = sample
                    .pointer("/video/uri")
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let Ok(file_id) = file_uri_id(uri) else {
                    continue;
                };
                if !files.insert(file_id.to_owned()) {
                    continue;
                }
                mutations.push(ResourceMutation::Save {
                    kind: "file",
                    id: file_id.to_owned(),
                    summary: serde_json::json!({"name": format!("files/{file_id}"), "uri": uri}),
                });
            }
        }
    }
    Ok(mutations)
}

fn save_file(file: gemini::File) -> Result<ResourceMutation, ChannelError> {
    let name = file
        .name
        .as_deref()
        .ok_or_else(|| observe("file resource has no name"))?;
    let id = resource_tail(name, "files")?.to_owned();
    let summary = serde_json::to_value(file).map_err(json_error)?;
    Ok(ResourceMutation::Save {
        kind: "file",
        id,
        summary,
    })
}

fn check_mutation_id(
    request: Option<(&'static str, &str)>,
    mutation: &ResourceMutation,
) -> Result<(), ChannelError> {
    let ResourceMutation::Save { id, .. } = mutation else {
        return Err(observe("retrieve did not produce a save mutation"));
    };
    if request.is_some_and(|(_, request_id)| request_id != id) {
        Err(observe("file response id differs from request"))
    } else {
        Ok(())
    }
}

fn resource_tail<'a>(name: &'a str, kind: &str) -> Result<&'a str, ChannelError> {
    let (prefix, id) = name
        .rsplit_once('/')
        .ok_or_else(|| observe("resource name has no slash"))?;
    if prefix.rsplit('/').next() != Some(kind) || id.is_empty() {
        return Err(observe("resource name has an unexpected shape"));
    }
    Ok(id)
}

fn file_uri_id(uri: &str) -> Result<&str, ChannelError> {
    let path = uri.split('?').next().unwrap_or(uri);
    let tail = path
        .rsplit_once("/files/")
        .map(|(_, id)| id)
        .or_else(|| path.strip_prefix("files/"))
        .ok_or_else(|| observe("generated video URI has no file id"))?;
    let id = tail.strip_suffix(":download").unwrap_or(tail);
    if id.is_empty() || id.contains('/') {
        return Err(observe("generated video URI has an invalid file id"));
    }
    Ok(id)
}

fn json_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Observe(error.to_string())
}

fn observe(message: &str) -> ChannelError {
    ChannelError::Observe(message.into())
}
