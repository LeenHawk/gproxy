use gproxy_channel_api::{ResourceCtx, ResourceMutation};
use gproxy_protocol::Affinity;

pub(super) fn mutations(ctx: ResourceCtx<'_>) -> Vec<ResourceMutation> {
    let Affinity::Resource(kind) = ctx.key.operation.spec().affinity else {
        return Vec::new();
    };
    ctx.response_headers
        .get(http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|location| {
            let id = location.rsplit('/').find(|part| !part.is_empty())?;
            Some(ResourceMutation::Save {
                kind,
                id: id.to_owned(),
                summary: serde_json::json!({"id": id, "location": location}),
            })
        })
        .into_iter()
        .collect()
}
