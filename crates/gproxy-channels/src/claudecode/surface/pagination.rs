use gproxy_channel_api::{ChannelError, Page, SurfaceServices};
use serde_json::{Value, json};

use super::helpers::decode_component;

pub(super) async fn list_resources(
    services: &SurfaceServices<'_>,
    kind: &'static str,
    query: Option<&str>,
) -> Result<ResourcePage, ChannelError> {
    let pairs = query_pairs(query);
    let limit = pair_value(&pairs, "limit")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(20)
        .clamp(1, 1_000);
    let page = services
        .bindings
        .list(
            services.provider.id,
            services.identity.user_id,
            kind,
            Page {
                cursor: pair_value(&pairs, "page").map(str::to_owned),
                limit,
            },
        )
        .await
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    let mut resources = page
        .items
        .into_iter()
        .filter_map(|binding| binding.summary.get("resource").cloned())
        .collect::<Vec<_>>();
    let ids = pairs
        .iter()
        .filter_map(|(key, value)| (key == "ids[]").then_some(value.as_str()))
        .collect::<Vec<_>>();
    if !ids.is_empty() {
        resources.retain(|resource| {
            resource
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| ids.contains(&id))
        });
    }
    if let Some(source) = pair_value(&pairs, "source") {
        resources.retain(|resource| {
            resource.pointer("/source/type").and_then(Value::as_str) == Some(source)
        });
    }
    Ok(ResourcePage {
        items: resources,
        next_cursor: page.next_cursor,
    })
}

pub(super) async fn list_all_resources(
    services: &SurfaceServices<'_>,
    kind: &'static str,
) -> Result<Vec<Value>, ChannelError> {
    let mut cursor = None;
    let mut resources = Vec::new();
    loop {
        let page = services
            .bindings
            .list(
                services.provider.id,
                services.identity.user_id,
                kind,
                Page {
                    cursor,
                    limit: 1_000,
                },
            )
            .await
            .map_err(|error| ChannelError::Prepare(error.to_string()))?;
        resources.extend(
            page.items
                .into_iter()
                .filter_map(|binding| binding.summary.get("resource").cloned()),
        );
        let Some(next_cursor) = page.next_cursor else {
            return Ok(resources);
        };
        cursor = Some(next_cursor);
    }
}

pub(super) struct ResourcePage {
    pub(super) items: Vec<Value>,
    pub(super) next_cursor: Option<String>,
}

pub(super) fn paginate(page: ResourcePage) -> Value {
    json!({ "data": page.items, "next_page": page.next_cursor })
}

fn query_pairs(query: Option<&str>) -> Vec<(String, String)> {
    query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (decode_component(key), decode_component(value))
        })
        .collect()
}

fn pair_value<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value.as_str()))
}
