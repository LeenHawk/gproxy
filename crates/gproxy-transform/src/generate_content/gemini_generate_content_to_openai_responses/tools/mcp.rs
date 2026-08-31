use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(super) fn convert(server: gemini::McpServer) -> Result<openai::ResponseTool, TransformError> {
    let label = server
        .name
        .ok_or_else(|| TransformError::shape("Gemini MCP server", "name is missing"))?;
    let (server_url, headers) = match server.streamable_http_transport {
        None => (None, None),
        Some(transport) => (transport.url, transport.headers),
    };
    Ok(openai::ResponseTool::Mcp {
        server_label: label,
        allowed_tools: None,
        authorization: None,
        connector_id: None,
        defer_loading: None,
        server_url,
        headers,
        require_approval: None,
        server_description: None,
        tunnel_id: None,
        allowed_callers: None,
        rest: Default::default(),
    })
}
