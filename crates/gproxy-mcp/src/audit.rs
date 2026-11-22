//! One tool call, as the trail records it.

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The server ran the tool and it succeeded.
    Ok,
    /// The server ran the tool and reported `isError`.
    ToolError,
    /// The server asked the caller for more input before finishing.
    InputRequired,
    /// The caller's scopes do not allow the tool.
    Refused,
    /// The call did not get an answer from the server.
    Failed,
}

/// The `detail` of a tool call's audit row. Who called is in the row's own
/// actor columns.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub server: String,
    pub tool: String,
    /// None when the call was refused before a credential was chosen.
    pub credential_id: Option<String>,
    pub duration_ms: u64,
    pub outcome: Outcome,
    /// Present only when the instance keeps payloads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
}
