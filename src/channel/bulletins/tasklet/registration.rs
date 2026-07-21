use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use super::{auth, registration_support as support, session};
use crate::http::client::{ClientError, UpstreamClient};

const SETUP_TIMEOUT: Duration = Duration::from_secs(120);
const BRIDGE_TOOL: &str = "gproxy_call_client_tool";

pub async fn ensure(
    client: &Arc<dyn UpstreamClient>,
    base: &str,
    token: &str,
    workspace: &str,
    timezone: &str,
    config: &auth::McpConfig,
) -> Result<(), ClientError> {
    let cache_key = format!(
        "{workspace}\0{}\0{}",
        config.url,
        crate::util::api_key::key_digest(&config.api_key)
    );
    if support::is_fresh(&cache_key) {
        return Ok(());
    }
    let lock = support::lock(&cache_key);
    let _guard = lock.lock().await;
    if support::is_fresh(&cache_key) {
        return Ok(());
    }

    let connection_id = match find_connection(client, base, token, workspace, &config.url).await? {
        Some(id) => id,
        None => create_connection(client, base, token, workspace, timezone, config).await?,
    };
    grant_bridge_tool(client, base, token, workspace, &connection_id).await?;
    support::mark_ready(cache_key);
    Ok(())
}

async fn create_connection(
    client: &Arc<dyn UpstreamClient>,
    base: &str,
    token: &str,
    workspace: &str,
    timezone: &str,
    config: &auth::McpConfig,
) -> Result<String, ClientError> {
    let body = json!({
        "agentId": "new",
        "message": format!(
            "Create a new MCP connection for {}. Open the MCP server setup form and wait for configuration.",
            config.url
        ),
        "timezone": timezone,
        "fileIds": [],
        "workspaceId": workspace,
        "agentConfig": {"preview": true},
    });
    let response = session::send_json(client, base, token, "/api/sendChatMessage", &body).await?;
    let value = support::success_json(response, "Tasklet MCP setup agent")?;
    let agent_id = support::string_at(&value, "/agentId", "setup agentId")?;
    let mut socket = session::open_socket(client, base, token).await?;
    socket
        .send_text(json!({"type":"startSync","agentId":agent_id}).to_string())
        .await?;
    socket
        .send_text(json!({"type":"subscribeBlocks","runId":agent_id,"pageSize":100}).to_string())
        .await?;
    let block_id = tokio::time::timeout(SETUP_TIMEOUT, async {
        loop {
            let text = socket.recv_text().await.ok_or_else(|| {
                ClientError::Transport("Tasklet MCP setup websocket closed".into())
            })??;
            let frame: Value = serde_json::from_str(&text).map_err(|error| {
                ClientError::Transport(format!("Tasklet MCP setup websocket JSON: {error}"))
            })?;
            if let Some(block_id) = support::pending_setup_block(&frame) {
                return Ok::<String, ClientError>(block_id);
            }
        }
    })
    .await
    .map_err(|_| ClientError::Transport("Tasklet MCP setup form timeout".into()))??;

    let setup = json!({
        "agentId": agent_id,
        "blockId": block_id,
        "config": {
            "displayName": "gproxy",
            "serverUrl": config.url,
            "customHeaders": [{"name":"X-API-Key","value":config.api_key}],
        }
    });
    let response = session::send_json(client, base, token, "/api/setupMcpServer", &setup).await?;
    let value = support::success_json(response, "Tasklet MCP setup")?;
    if value.get("status").and_then(Value::as_str) != Some("success") {
        return Err(ClientError::Config(
            "Tasklet rejected the gproxy MCP URL or API key".into(),
        ));
    }
    for _ in 0..20 {
        if let Some(id) = find_connection(client, base, token, workspace, &config.url).await? {
            return Ok(id);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(ClientError::Transport(
        "Tasklet MCP connection was not visible after setup".into(),
    ))
}

async fn find_connection(
    client: &Arc<dyn UpstreamClient>,
    base: &str,
    token: &str,
    workspace: &str,
    url: &str,
) -> Result<Option<String>, ClientError> {
    let response = session::send_json(
        client,
        base,
        token,
        "/api/getAllConnections",
        &json!({"workspaceId":workspace}),
    )
    .await?;
    let value = support::success_json(response, "Tasklet connections")?;
    Ok(value
        .get("connections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|connection| {
            connection.pointer("/details/type").and_then(Value::as_str) == Some("mcp")
                && connection
                    .pointer("/details/serverUrl")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate.trim_end_matches('/') == url)
        })
        .and_then(|connection| connection.get("connectionId"))
        .and_then(Value::as_str)
        .map(str::to_owned))
}

async fn grant_bridge_tool(
    client: &Arc<dyn UpstreamClient>,
    base: &str,
    token: &str,
    workspace: &str,
    connection_id: &str,
) -> Result<(), ClientError> {
    let body = json!({
        "workspaceId": workspace,
        "connectionId": connection_id,
        "grantedToolNames": [BRIDGE_TOOL],
    });
    let response = session::send_json(
        client,
        base,
        token,
        "/api/updateConnectionToolPermissions",
        &body,
    )
    .await?;
    let value = support::success_json(response, "Tasklet MCP permissions")?;
    if value.get("status").and_then(Value::as_str) == Some("success") {
        Ok(())
    } else {
        Err(ClientError::Transport(
            "Tasklet did not grant the gproxy MCP tool".into(),
        ))
    }
}
