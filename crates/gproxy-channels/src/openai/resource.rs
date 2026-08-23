use gproxy_channel_api::{ChannelError, ResourceCtx, ResourceMutation, UsageCtx};
use gproxy_protocol::Affinity;
use gproxy_protocol::openai;

pub(super) fn settlement_ready(ctx: UsageCtx<'_>) -> Result<bool, ChannelError> {
    let video: openai::video::Video = serde_json::from_slice(ctx.response_body).map_err(observe)?;
    Ok(video.error.is_none()
        && matches!(
            video.status,
            openai::video::VideoStatus::Known(openai::video::KnownVideoStatus::Completed)
        ))
}

pub(super) fn mutations(ctx: ResourceCtx<'_>) -> Result<Vec<ResourceMutation>, ChannelError> {
    let Affinity::Resource(kind) = ctx.key.operation.spec().affinity else {
        return Ok(Vec::new());
    };
    use gproxy_protocol::Operation::*;
    match ctx.key.operation {
        CreateFile | RetrieveFile => {
            let file: openai::files::FileObject =
                serde_json::from_slice(ctx.response_body).map_err(observe)?;
            check_request_id(ctx.request_resource, &file.id)?;
            let id = file.id.clone();
            let summary = serde_json::to_value(file).map_err(observe)?;
            Ok(vec![save(kind, id, summary)])
        }
        ListFiles => {
            let list: openai::files::ListFilesResponse =
                serde_json::from_slice(ctx.response_body).map_err(observe)?;
            list.data
                .into_iter()
                .map(|file| {
                    let id = file.id.clone();
                    let summary = serde_json::to_value(file).map_err(observe)?;
                    Ok(save(kind, id, summary))
                })
                .collect()
        }
        DeleteFile => {
            let deleted: openai::files::DeleteFileResponse =
                serde_json::from_slice(ctx.response_body).map_err(observe)?;
            delete(kind, ctx.request_resource, &deleted.id, deleted.deleted)
        }
        CreateVideo | RemixVideo | EditVideo | ExtendVideo => {
            let video: openai::video::Video =
                serde_json::from_slice(ctx.response_body).map_err(observe)?;
            let id = video.id.clone();
            let summary = serde_json::to_value(video).map_err(observe)?;
            Ok(vec![save(kind, id, summary)])
        }
        RetrieveVideo => {
            let video: openai::video::Video =
                serde_json::from_slice(ctx.response_body).map_err(observe)?;
            check_request_id(ctx.request_resource, &video.id)?;
            let id = video.id.clone();
            let summary = serde_json::to_value(video).map_err(observe)?;
            Ok(vec![save(kind, id, summary)])
        }
        ListVideos => {
            let list: openai::video::VideoListResponse =
                serde_json::from_slice(ctx.response_body).map_err(observe)?;
            list.data
                .into_iter()
                .map(|video| {
                    let id = video.id.clone();
                    let summary = serde_json::to_value(video).map_err(observe)?;
                    Ok(save(kind, id, summary))
                })
                .collect()
        }
        DeleteVideo => {
            let deleted: openai::video::VideoDeleteResponse =
                serde_json::from_slice(ctx.response_body).map_err(observe)?;
            delete(kind, ctx.request_resource, &deleted.id, deleted.deleted)
        }
        CreateVideoCharacter | GetVideoCharacter => {
            let character: openai::video::VideoCharacter =
                serde_json::from_slice(ctx.response_body).map_err(observe)?;
            check_request_id(ctx.request_resource, &character.id)?;
            let id = character.id.clone();
            let summary = serde_json::to_value(character).map_err(observe)?;
            Ok(vec![save(kind, id, summary)])
        }
        CreateRealtimeCall => location(kind, ctx.response_headers),
        RetrieveFileContent | DownloadVideoContent => Ok(Vec::new()),
        _ => Ok(Vec::new()),
    }
}

fn save(kind: &'static str, id: String, summary: serde_json::Value) -> ResourceMutation {
    ResourceMutation::Save { kind, id, summary }
}

fn observe(error: serde_json::Error) -> ChannelError {
    ChannelError::Observe(error.to_string())
}

fn delete(
    kind: &'static str,
    request: Option<(&'static str, &str)>,
    response_id: &str,
    deleted: bool,
) -> Result<Vec<ResourceMutation>, ChannelError> {
    if !deleted {
        return Err(ChannelError::Observe(
            "delete response did not confirm deletion".into(),
        ));
    }
    check_request_id(request, response_id)?;
    Ok(vec![ResourceMutation::Delete {
        kind,
        id: response_id.to_owned(),
    }])
}

fn check_request_id(
    request: Option<(&'static str, &str)>,
    response_id: &str,
) -> Result<(), ChannelError> {
    if request.is_some_and(|(_, request_id)| request_id != response_id) {
        Err(ChannelError::Observe(
            "response resource id differs from the requested id".into(),
        ))
    } else {
        Ok(())
    }
}

fn location(
    kind: &'static str,
    headers: &http::HeaderMap,
) -> Result<Vec<ResourceMutation>, ChannelError> {
    let Some(location) = headers
        .get(http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(Vec::new());
    };
    let path = location
        .parse::<http::Uri>()
        .map_err(|error| ChannelError::Observe(format!("invalid Location URI: {error}")))?;
    let id = path
        .path()
        .rsplit('/')
        .find(|part| !part.is_empty())
        .ok_or_else(|| ChannelError::Observe("Location has no resource id".into()))?;
    Ok(vec![ResourceMutation::Save {
        kind,
        id: id.to_owned(),
        summary: serde_json::json!({"id": id, "location": location}),
    }])
}
