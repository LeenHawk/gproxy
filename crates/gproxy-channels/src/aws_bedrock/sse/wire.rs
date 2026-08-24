use bytes::{Bytes, BytesMut};
use gproxy_channel_api::{ChannelError, Frame};
use serde_json::{Value, json};

pub(super) fn message_start() -> Result<Frame, ChannelError> {
    event(
        "message_start",
        json!({
            "type":"message_start",
            "message":{
                "id":"msg_bedrock",
                "type":"message",
                "role":"assistant",
                "content":[],
                "model":"aws-bedrock",
                "stop_reason":null,
                "stop_sequence":null,
                "usage":{}
            }
        }),
    )
}

pub(super) fn block_start(index: u64, kind: BlockKind) -> Result<Frame, ChannelError> {
    let block = match kind {
        BlockKind::Text => json!({"type":"text","text":""}),
        BlockKind::Thinking => json!({"type":"thinking","thinking":""}),
        BlockKind::Tool { id, name } => {
            json!({"type":"tool_use","id":id,"name":name,"input":{}})
        }
    };
    event(
        "content_block_start",
        json!({"type":"content_block_start","index":index,"content_block":block}),
    )
}

pub(super) fn block_delta(index: u64, delta: Value) -> Result<Frame, ChannelError> {
    event(
        "content_block_delta",
        json!({"type":"content_block_delta","index":index,"delta":delta}),
    )
}

pub(super) fn block_stop(index: u64) -> Result<Frame, ChannelError> {
    event(
        "content_block_stop",
        json!({"type":"content_block_stop","index":index}),
    )
}

pub(super) fn message_end(stop_reason: Value, usage: Value) -> Result<Vec<Frame>, ChannelError> {
    Ok(vec![
        event(
            "message_delta",
            json!({
                "type":"message_delta",
                "delta":{"stop_reason":stop_reason,"stop_sequence":null},
                "usage":usage
            }),
        )?,
        event("message_stop", json!({"type":"message_stop"}))?,
    ])
}

pub(super) fn error(kind: &str, message: &str) -> Result<Frame, ChannelError> {
    event(
        "error",
        json!({"type":"error","error":{"type":kind,"message":message}}),
    )
}

fn event(name: &str, value: Value) -> Result<Frame, ChannelError> {
    let typed: gproxy_protocol::claude::StreamEvent = serde_json::from_value(value)
        .map_err(|error| ChannelError::Decode(format!("Claude SSE event: {error}")))?;
    let json = serde_json::to_vec(&typed)
        .map_err(|error| ChannelError::Decode(format!("Claude SSE event: {error}")))?;
    let mut bytes = BytesMut::with_capacity(name.len() + json.len() + 16);
    bytes.extend_from_slice(b"event: ");
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(b"\ndata: ");
    bytes.extend_from_slice(&json);
    bytes.extend_from_slice(b"\n\n");
    Ok(Frame(Bytes::from(bytes)))
}

pub(super) enum BlockKind {
    Text,
    Thinking,
    Tool { id: String, name: String },
}
