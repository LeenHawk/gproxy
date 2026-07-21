use std::collections::BTreeMap;

use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use rmcp::{ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::Value;

use super::bridge;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DelegateRequest {
    /// Opaque, single-use turn identifier supplied in the Tasklet prompt.
    turn_id: String,
    /// Exact client tool name from the supplied tool catalogue.
    name: String,
    /// Arguments matching that client tool's JSON schema.
    arguments: BTreeMap<String, Value>,
}

#[derive(Clone)]
pub(crate) struct TaskletMcp {
    tools: ToolRouter<Self>,
}

impl TaskletMcp {
    fn new() -> Self {
        Self {
            tools: Self::tools(),
        }
    }
}

#[tool_router(router = tools)]
impl TaskletMcp {
    #[tool(
        name = "gproxy_call_client_tool",
        description = "Delegate one tool call to the client connected through gproxy. Use only when the current task includes a gproxy turn_id and client tool catalogue."
    )]
    async fn call_client_tool(
        &self,
        Parameters(request): Parameters<DelegateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        bridge::dispatch(
            &request.turn_id,
            request.name,
            Value::Object(request.arguments.into_iter().collect()),
        )
        .await
        .map_err(|message| ErrorData::invalid_params(message, None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            "The tool call was delegated to the original client. Stop this turn now.",
        )]))
    }
}

#[tool_handler(router = self.tools)]
impl ServerHandler for TaskletMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "A private bridge from Tasklet to tool-capable clients using gproxy.",
        )
    }
}

pub(crate) fn service() -> StreamableHttpService<TaskletMcp, LocalSessionManager> {
    StreamableHttpService::new(
        || Ok(TaskletMcp::new()),
        Default::default(),
        StreamableHttpServerConfig::default()
            .with_stateful_mode(false)
            .with_json_response(true)
            .with_sse_keep_alive(None),
    )
}
