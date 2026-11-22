//! `/connect/mcp`: the MCP gateway's route.
//!
//! The path says which servers are meant, the query may narrow the
//! aggregated endpoint, and the mount is respected: on a provider mount the
//! endpoint is that provider and nothing else.

use axum::response::{IntoResponse, Response};
use gproxy_app::{AppError, McpEndpoint, McpRequest};
use gproxy_protocol::connection::Bytes;
use gproxy_seaorm::BatchConnectionTrait;

use crate::{HostState, Mount, error::ErrorResponse};

const PREFIX: &str = "/connect/mcp";

/// Whether `remainder` is the gateway's own path rather than an upstream's.
pub(super) fn is_path(remainder: &str) -> bool {
    remainder
        .strip_prefix(PREFIX)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The endpoint a path, its query and the mount name together.
fn endpoint(mount: &Mount, remainder: &str, query: Option<&str>) -> Result<McpEndpoint, AppError> {
    let not_found = || AppError::not_found("route", remainder.to_owned());
    let named = remainder
        .strip_prefix(PREFIX)
        .map(|rest| rest.trim_matches('/'))
        .filter(|name| !name.is_empty());
    if named.is_some_and(|name| name.contains('/')) {
        return Err(not_found());
    }
    match (mount, named) {
        (Mount::Namespace(_), _) => Err(not_found()),
        (Mount::Provider(provider), None) => Ok(McpEndpoint::Server(provider.clone())),
        (Mount::Provider(provider), Some(name)) if name != provider => Err(not_found()),
        (_, Some(name)) => Ok(McpEndpoint::Server(name.to_owned())),
        (Mount::Aggregated, None) => {
            let servers =
                serde_urlencoded::from_str::<Vec<(String, String)>>(query.unwrap_or_default())
                    .unwrap_or_default()
                    .into_iter()
                    .find(|(name, _)| name == "servers")
                    .map(|(_, list)| {
                        list.split(',')
                            .map(str::trim)
                            .filter(|name| !name.is_empty())
                            .map(str::to_owned)
                            .collect()
                    });
            Ok(McpEndpoint::All { servers })
        }
    }
}

pub(super) async fn handle<C>(
    state: &HostState<C>,
    mount: &Mount,
    remainder: &str,
    parts: http::request::Parts,
    body: Bytes,
    client_ip: String,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let caller = match super::authenticate(state.app(), &parts).await {
        Ok(caller) => caller,
        Err(error) => return ErrorResponse(error).into_response(),
    };
    let endpoint = match endpoint(mount, remainder, parts.uri.query()) {
        Ok(endpoint) => endpoint,
        Err(error) => return ErrorResponse(error).into_response(),
    };
    let request = McpRequest {
        endpoint,
        method: parts.method,
        headers: parts.headers,
        body,
        source_ip: Some(client_ip),
    };
    match state.app().mcp(&caller, request).await {
        Ok(reply) => {
            let mut response = match reply.body {
                Some(body) => axum::body::Body::from(body.to_string()).into_response(),
                None => ().into_response(),
            };
            *response.status_mut() = reply.status;
            response.headers_mut().extend(reply.headers);
            response
        }
        Err(error) => ErrorResponse(error).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(servers: Option<&[&str]>) -> McpEndpoint {
        McpEndpoint::All {
            servers: servers.map(|names| names.iter().map(|name| (*name).to_owned()).collect()),
        }
    }

    #[test]
    fn the_path_the_query_and_the_mount_name_the_servers() {
        let root = Mount::Aggregated;
        assert_eq!(endpoint(&root, "/connect/mcp", None).unwrap(), all(None));
        assert_eq!(
            endpoint(&root, "/connect/mcp", Some("servers=wiki,%20notion")).unwrap(),
            all(Some(&["wiki", "notion"]))
        );
        assert_eq!(
            endpoint(&root, "/connect/mcp/wiki", None).unwrap(),
            McpEndpoint::Server("wiki".to_owned())
        );
        assert!(endpoint(&root, "/connect/mcp/wiki/extra", None).is_err());

        let wiki = Mount::Provider("wiki".to_owned());
        assert_eq!(
            endpoint(&wiki, "/connect/mcp", Some("servers=notion")).unwrap(),
            McpEndpoint::Server("wiki".to_owned())
        );
        assert!(endpoint(&wiki, "/connect/mcp/notion", None).is_err());
        assert!(endpoint(&Mount::Namespace("acme".to_owned()), "/connect/mcp", None).is_err());

        assert!(is_path("/connect/mcp") && is_path("/connect/mcp/wiki"));
        assert!(!is_path("/connect/mcpx") && !is_path("/v1/connect/mcp"));
    }
}
