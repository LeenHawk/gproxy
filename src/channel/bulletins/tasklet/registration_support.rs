use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use http::StatusCode;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::http::client::ClientError;

const CHECK_TTL_MS: u64 = 300_000;

pub fn pending_setup_block(frame: &Value) -> Option<String> {
    let blocks = frame
        .get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .chain(
            frame
                .get("updates")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|updates| updates.values()),
        );
    for block in blocks {
        let arguments = block
            .get("arguments")
            .and_then(Value::as_str)
            .and_then(|text| serde_json::from_str::<Value>(text).ok());
        if block.get("type").and_then(Value::as_str) == Some("tool_use")
            && block.get("name").and_then(Value::as_str) == Some("invoke_tool")
            && arguments.as_ref().and_then(|value| value.get("toolName"))
                == Some(&Value::String("create_new_connections".into()))
            && arguments
                .as_ref()
                .and_then(|value| value.pointer("/args/connection/type"))
                == Some(&Value::String("mcp".into()))
            && block.pointer("/result/type").and_then(Value::as_str) == Some("pending")
            && block
                .pointer("/data/mcpServerSetup/completed")
                .and_then(Value::as_bool)
                == Some(false)
        {
            return block
                .get("blockId")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
    }
    None
}

pub fn success_json(
    response: http::Response<bytes::Bytes>,
    label: &str,
) -> Result<Value, ClientError> {
    if response.status() != StatusCode::OK {
        return Err(ClientError::Transport(format!(
            "{label} failed: {}",
            response.status()
        )));
    }
    serde_json::from_slice(response.body())
        .map_err(|error| ClientError::Transport(format!("{label} JSON: {error}")))
}

pub fn string_at(value: &Value, pointer: &str, label: &str) -> Result<String, ClientError> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| ClientError::Transport(format!("Tasklet response missing {label}")))
}

pub fn is_fresh(key: &str) -> bool {
    ready()
        .get(key)
        .is_some_and(|expires| *expires > crate::util::time::unix_now_ms())
}

pub fn mark_ready(key: String) {
    ready().insert(key, crate::util::time::unix_now_ms() + CHECK_TTL_MS);
}

pub fn lock(key: &str) -> Arc<Mutex<()>> {
    locks()
        .entry(key.to_owned())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn ready() -> &'static DashMap<String, u64> {
    static READY: OnceLock<DashMap<String, u64>> = OnceLock::new();
    READY.get_or_init(DashMap::new)
}

fn locks() -> &'static DashMap<String, Arc<Mutex<()>>> {
    static LOCKS: OnceLock<DashMap<String, Arc<Mutex<()>>>> = OnceLock::new();
    LOCKS.get_or_init(DashMap::new)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn finds_pending_create_mcp_connection_block() {
        let arguments = json!({
            "toolName":"create_new_connections",
            "args":{"connection":{"type":"mcp"}}
        })
        .to_string();
        let frame = json!({"updates":{"b_setup":{
            "type":"tool_use",
            "blockId":"b_setup",
            "name":"invoke_tool",
            "arguments":arguments,
            "result":{"type":"pending"},
            "data":{"mcpServerSetup":{"completed":false}}
        }}});
        assert_eq!(
            super::pending_setup_block(&frame).as_deref(),
            Some("b_setup")
        );
    }
}
