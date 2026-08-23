use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::common::tools::empty_response_tool;

pub(super) fn convert(server: gemini::McpServer) -> Result<openai::ResponseTool, TransformError> {
    super::definitions::ensure_empty(&server.rest, "Gemini MCP server")?;
    let label = server
        .name
        .ok_or_else(|| TransformError::shape("Gemini MCP server", "name is missing"))?;
    let (server_url, headers) = match server.streamable_http_transport {
        None => (None, None),
        Some(transport) => {
            if transport.timeout.is_some()
                || transport.sse_read_timeout.is_some()
                || transport.terminate_on_close.is_some()
                || !transport.rest.is_empty()
            {
                return Err(TransformError::unsupported(
                    "Gemini MCP transport",
                    "timeouts, terminateOnClose, or extension fields",
                ));
            }
            if transport.url.is_none() && transport.headers.is_none() {
                return Err(TransformError::unsupported(
                    "Gemini MCP transport",
                    "empty transport object",
                ));
            }
            (transport.url, transport.headers)
        }
    };
    Ok(openai::ResponseTool {
        type_: openai::ToolType::Mcp,
        server_label: Some(label),
        server_url,
        headers,
        ..empty_response_tool()
    })
}
