use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use dashmap::DashMap;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::protocol::openai::ChatTool;

const TURN_TTL_MS: u64 = 2_100_000;
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
enum ToolKind {
    Function,
    Custom,
}

struct ActiveTurn {
    tools: Arc<HashMap<String, ToolKind>>,
    sender: mpsc::Sender<ToolInvocation>,
    expires_ms: u64,
}

pub(super) enum InvocationPayload {
    Function(String),
    Custom(String),
}

pub(super) struct ToolInvocation {
    pub name: String,
    pub payload: InvocationPayload,
    accepted: oneshot::Sender<()>,
}

pub(super) struct Turn {
    id: String,
    receiver: mpsc::Receiver<ToolInvocation>,
}

impl Turn {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub async fn recv(&mut self) -> Option<ToolInvocation> {
        self.receiver.recv().await
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        turns().remove(&self.id);
    }
}

pub(super) fn register(tools: &[ChatTool]) -> Option<Turn> {
    let tools = tools
        .iter()
        .map(|tool| match tool {
            ChatTool::Function { function, .. } => (function.name.clone(), ToolKind::Function),
            ChatTool::Custom { custom, .. } => (custom.name.clone(), ToolKind::Custom),
        })
        .collect::<HashMap<_, _>>();
    if tools.is_empty() {
        return None;
    }
    let now = crate::util::time::unix_now_ms();
    turns().retain(|_, active| active.expires_ms > now);
    let id = URL_SAFE_NO_PAD.encode(crate::util::rand::bytes::<24>());
    let (sender, receiver) = mpsc::channel(1);
    turns().insert(
        id.clone(),
        ActiveTurn {
            tools: Arc::new(tools),
            sender,
            expires_ms: now + TURN_TTL_MS,
        },
    );
    Some(Turn { id, receiver })
}

pub(super) async fn dispatch(turn_id: &str, name: String, arguments: Value) -> Result<(), String> {
    let kind = {
        let active = turns()
            .get(turn_id)
            .ok_or_else(|| "unknown or expired gproxy turn_id".to_owned())?;
        if active.expires_ms <= crate::util::time::unix_now_ms() {
            drop(active);
            turns().remove(turn_id);
            return Err("expired gproxy turn_id".into());
        }
        active
            .tools
            .get(&name)
            .copied()
            .ok_or_else(|| format!("tool {name:?} is not available for this turn"))?
    };
    let (_, active) = turns()
        .remove(turn_id)
        .ok_or_else(|| "gproxy turn_id was already claimed".to_owned())?;
    let payload = match kind {
        ToolKind::Function => InvocationPayload::Function(
            serde_json::to_string(&arguments)
                .map_err(|error| format!("tool arguments JSON: {error}"))?,
        ),
        ToolKind::Custom => InvocationPayload::Custom(
            arguments
                .get("input")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| arguments.to_string()),
        ),
    };
    let (accepted, acknowledgement) = oneshot::channel();
    active
        .sender
        .send(ToolInvocation {
            name,
            payload,
            accepted,
        })
        .await
        .map_err(|_| "the downstream gproxy request has closed".to_owned())?;
    tokio::time::timeout(ACCEPT_TIMEOUT, acknowledgement)
        .await
        .map_err(|_| "downstream gproxy request did not accept the tool call".to_owned())?
        .map_err(|_| "downstream gproxy request rejected the tool call".to_owned())
}

pub(super) fn into_parts(
    invocation: ToolInvocation,
) -> (String, InvocationPayload, oneshot::Sender<()>) {
    let ToolInvocation {
        name,
        payload,
        accepted,
    } = invocation;
    (name, payload, accepted)
}

fn turns() -> &'static DashMap<String, ActiveTurn> {
    static TURNS: OnceLock<DashMap<String, ActiveTurn>> = OnceLock::new();
    TURNS.get_or_init(DashMap::new)
}
