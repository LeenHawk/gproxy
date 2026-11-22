//! Towards callers: what a POST to the gateway's MCP endpoint asks, and the
//! answers in the shape the caller's protocol revision expects.
//!
//! Both lifecycles are accepted. A caller on 2026-07-28 sends self-contained
//! requests and is recognised by the revision it names; any other caller
//! opens with `initialize` and is given a session id. The gateway serves
//! tools only, and answers every request with one JSON body.

use http::{HeaderMap, HeaderValue, StatusCode, header};
use serde_json::{Value, json};

use crate::{
    client::{Answer, STATELESS_VERSION},
    tool::Nature,
};

const SESSION_ID: &str = "mcp-session-id";
const PROTOCOL_VERSION: &str = "mcp-protocol-version";
/// The revisions with an `initialize` handshake, oldest first.
const SESSION_VERSIONS: [&str; 4] = ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
/// How long a 2026-07-28 caller may reuse a tool list. Short, because the
/// list follows the caller's scopes.
const TOOLS_TTL_MS: u64 = 60_000;

const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_REQUEST: i64 = -32600;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;
const PARSE_ERROR: i64 = -32700;

/// Which lifecycle the caller follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Revision {
    /// 2026-07-28 and later.
    Stateless,
    /// `initialize`, then a session id.
    Session,
}

#[derive(Debug, PartialEq)]
pub enum Inbound {
    Initialize {
        /// The revision the gateway answers with.
        version: &'static str,
    },
    Discover,
    Ping,
    ListTools,
    CallTool {
        name: String,
        /// The request's whole `params`, handed on as they are.
        params: Value,
    },
    /// A request for something the gateway does not serve.
    Unknown {
        method: String,
    },
    /// A notification, or the answer to a request the gateway never sent.
    Notification,
}

#[derive(Debug)]
pub struct Request {
    /// The JSON-RPC id to answer with; null for a notification.
    pub id: Value,
    pub revision: Revision,
    /// The caller's `Mcp-Session-Id`, when it sent one.
    pub session: Option<String>,
    /// The client capabilities the request declared for itself.
    pub capabilities: Option<Value>,
    pub inbound: Inbound,
}

/// One HTTP answer.
#[derive(Debug)]
pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Option<Value>,
}

impl Reply {
    fn json(status: StatusCode, body: Value) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Self {
            status,
            headers,
            body: Some(body),
        }
    }

    pub fn result(id: &Value, result: Value) -> Self {
        Self::json(
            StatusCode::OK,
            json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        )
    }

    fn error(status: StatusCode, id: &Value, code: i64, message: impl Into<String>) -> Self {
        Self::json(
            status,
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code, "message": message.into() },
            }),
        )
    }

    /// A notification was taken; there is nothing to say.
    pub fn accepted() -> Self {
        Self {
            status: StatusCode::ACCEPTED,
            headers: HeaderMap::new(),
            body: None,
        }
    }

    /// The server's own JSON-RPC error, handed on.
    pub fn upstream_error(id: &Value, error: Value) -> Self {
        Self::json(
            StatusCode::OK,
            json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        )
    }

    pub fn unknown_method(id: &Value, method: &str) -> Self {
        Self::error(
            StatusCode::OK,
            id,
            METHOD_NOT_FOUND,
            format!("method `{method}` is not served; this endpoint offers tools"),
        )
    }

    /// A tool the caller cannot use is answered like one that does not exist.
    pub fn unknown_tool(id: &Value, name: &str) -> Self {
        Self::error(
            StatusCode::OK,
            id,
            INVALID_PARAMS,
            format!("Unknown tool: {name}"),
        )
    }

    pub fn invalid(id: &Value, message: impl Into<String>) -> Self {
        Self::error(StatusCode::BAD_REQUEST, id, INVALID_REQUEST, message)
    }

    /// The server behind the gateway did not answer usably. Deliberately a
    /// plain 502 with no challenge: a caller must not conclude that it
    /// should authenticate to anything itself.
    pub fn bad_gateway(id: &Value, message: impl Into<String>) -> Self {
        Self::error(StatusCode::BAD_GATEWAY, id, INTERNAL_ERROR, message)
    }

    /// The caller's session id on the answer to its `initialize`.
    pub fn with_session(mut self, session: &str) -> Self {
        if let Ok(value) = HeaderValue::from_str(session) {
            self.headers.insert(SESSION_ID, value);
        }
        self
    }
}

fn is_stateless(version: &str) -> bool {
    version >= STATELESS_VERSION
}

/// Read one POST. `Err` is the answer to a body that is not a JSON-RPC message.
pub fn read(headers: &HeaderMap, body: &[u8]) -> Result<Request, Box<Reply>> {
    let message: Value = serde_json::from_slice(body).map_err(|error| {
        Box::new(Reply::error(
            StatusCode::BAD_REQUEST,
            &Value::Null,
            PARSE_ERROR,
            error.to_string(),
        ))
    })?;
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let text = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let session = text(SESSION_ID);
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        if message.get("result").is_some() || message.get("error").is_some() {
            return Ok(Request {
                id,
                revision: Revision::Session,
                session,
                capabilities: None,
                inbound: Inbound::Notification,
            });
        }
        return Err(Box::new(Reply::invalid(&id, "not a JSON-RPC request")));
    };
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    let meta = |key: &str| params.pointer(&format!("/_meta/io.modelcontextprotocol~1{key}"));
    let named = meta("protocolVersion")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| text(PROTOCOL_VERSION));
    let revision = match named.as_deref() {
        Some(version) if is_stateless(version) => Revision::Stateless,
        _ => Revision::Session,
    };
    let inbound = if id.is_null() {
        Inbound::Notification
    } else {
        match method {
            "initialize" => {
                let requested = params.get("protocolVersion").and_then(Value::as_str);
                Inbound::Initialize {
                    // A revision the gateway does not know is answered with
                    // the newest it has a handshake for.
                    version: SESSION_VERSIONS
                        .into_iter()
                        .find(|known| Some(*known) == requested)
                        .unwrap_or(SESSION_VERSIONS[SESSION_VERSIONS.len() - 1]),
                }
            }
            "server/discover" => Inbound::Discover,
            "ping" => Inbound::Ping,
            "tools/list" => Inbound::ListTools,
            "tools/call" => match params.get("name").and_then(Value::as_str) {
                Some(name) => Inbound::CallTool {
                    name: name.to_owned(),
                    params: params.clone(),
                },
                None => return Err(Box::new(Reply::invalid(&id, "tools/call names no tool"))),
            },
            other => Inbound::Unknown {
                method: other.to_owned(),
            },
        }
    };
    Ok(Request {
        id,
        revision: match inbound {
            Inbound::Initialize { .. } => Revision::Session,
            _ => revision,
        },
        session,
        capabilities: meta("clientCapabilities").cloned(),
        inbound,
    })
}

fn server_info() -> Value {
    json!({ "name": "gproxy", "version": env!("CARGO_PKG_VERSION") })
}

pub fn initialize(id: &Value, version: &str) -> Reply {
    Reply::result(
        id,
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": {} },
            "serverInfo": server_info(),
        }),
    )
}

pub fn discover(id: &Value) -> Reply {
    let mut versions = SESSION_VERSIONS.to_vec();
    versions.push(STATELESS_VERSION);
    Reply::result(
        id,
        json!({
            "resultType": "complete",
            "supportedVersions": versions,
            "capabilities": { "tools": {} },
            "ttlMs": 3_600_000,
            "cacheScope": "public",
            "_meta": { "io.modelcontextprotocol/serverInfo": server_info() },
        }),
    )
}

pub fn pong(id: &Value, revision: Revision) -> Reply {
    Reply::result(
        id,
        match revision {
            Revision::Stateless => json!({ "resultType": "complete" }),
            Revision::Session => json!({}),
        },
    )
}

/// One tool as the gateway offers it: the server's own description under the
/// name callers use, with the nature the gateway settled on.
pub fn offered(tool: &crate::Tool, name: &str, nature: Nature) -> Value {
    let mut offered = serde_json::to_value(tool).expect("a tool serializes");
    offered["name"] = json!(name);
    let annotations = &mut offered["annotations"];
    if !annotations.is_object() {
        *annotations = json!({});
    }
    annotations["readOnlyHint"] = json!(nature == Nature::Read);
    annotations["destructiveHint"] = json!(nature == Nature::Destructive);
    offered
}

pub fn tools(id: &Value, revision: Revision, tools: Vec<Value>) -> Reply {
    Reply::result(
        id,
        match revision {
            Revision::Stateless => json!({
                "resultType": "complete",
                "tools": tools,
                "ttlMs": TOOLS_TTL_MS,
                "cacheScope": "private",
            }),
            Revision::Session => json!({ "tools": tools }),
        },
    )
}

/// What the result of a tool call is, whatever the revision it came in.
pub fn result_type(result: &Value) -> &str {
    result
        .get("resultType")
        .and_then(Value::as_str)
        .unwrap_or("complete")
}

/// The server's answer to a tool call, for a caller on `revision`.
pub fn call(id: &Value, revision: Revision, answer: Answer) -> Reply {
    let mut result = match answer {
        Ok(result) => result,
        Err(error) => return Reply::upstream_error(id, error),
    };
    match revision {
        Revision::Stateless => {
            if result.get("resultType").is_none() {
                result["resultType"] = json!("complete");
            }
        }
        Revision::Session => {
            // `input_required` and `task` are answers only 2026-07-28 knows.
            if result_type(&result) != "complete" {
                return Reply::bad_gateway(
                    id,
                    "the tool needs a client that speaks MCP 2026-07-28",
                );
            }
            if let Some(fields) = result.as_object_mut() {
                fields.remove("resultType");
            }
        }
    }
    Reply::result(id, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(headers: &[(&'static str, &'static str)], body: Value) -> Result<Request, Box<Reply>> {
        let headers = headers
            .iter()
            .map(|(name, value)| {
                (
                    header::HeaderName::from_static(name),
                    HeaderValue::from_static(value),
                )
            })
            .collect();
        read(&headers, body.to_string().as_bytes())
    }

    #[test]
    fn a_caller_is_stateless_when_its_request_says_so() {
        let by_meta = post(
            &[],
            json!({
                "jsonrpc": "2.0", "id": 7, "method": "tools/list",
                "params": { "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": { "elicitation": {} },
                } },
            }),
        )
        .unwrap();
        assert_eq!(by_meta.revision, Revision::Stateless);
        assert_eq!(by_meta.capabilities, Some(json!({ "elicitation": {} })));
        assert_eq!(by_meta.inbound, Inbound::ListTools);

        let by_header = post(
            &[("mcp-protocol-version", "2026-07-28")],
            json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }),
        )
        .unwrap();
        assert_eq!(by_header.revision, Revision::Stateless);

        let session = post(
            &[
                ("mcp-protocol-version", "2025-11-25"),
                ("mcp-session-id", "s1"),
            ],
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .unwrap();
        assert_eq!(session.revision, Revision::Session);
        assert_eq!(session.session.as_deref(), Some("s1"));
    }

    #[test]
    fn initialize_is_answered_with_a_revision_that_has_a_handshake() {
        let version = |requested: &str| {
            let request = post(
                &[],
                json!({
                    "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": { "protocolVersion": requested, "capabilities": {} },
                }),
            )
            .unwrap();
            assert_eq!(request.revision, Revision::Session);
            match request.inbound {
                Inbound::Initialize { version } => version,
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(version("2025-06-18"), "2025-06-18");
        assert_eq!(version("2026-07-28"), "2025-11-25");
        assert_eq!(version("1999-01-01"), "2025-11-25");
    }

    #[test]
    fn messages_without_an_id_need_no_answer() {
        let request = post(
            &[],
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        )
        .unwrap();
        assert_eq!(request.inbound, Inbound::Notification);
        assert!(post(&[], json!([])).is_err());
    }

    #[test]
    fn a_result_is_shaped_for_the_callers_revision() {
        let id = json!(1);
        let plain = json!({ "content": [] });
        let stateless = call(&id, Revision::Stateless, Ok(plain.clone()));
        assert_eq!(
            stateless.body.unwrap()["result"],
            json!({ "content": [], "resultType": "complete" })
        );

        let complete = json!({ "content": [], "resultType": "complete" });
        let session = call(&id, Revision::Session, Ok(complete));
        assert_eq!(session.body.unwrap()["result"], plain);

        let asks = json!({ "resultType": "input_required", "inputRequests": {} });
        let passed = call(&id, Revision::Stateless, Ok(asks.clone()));
        assert_eq!(passed.body.unwrap()["result"], asks);
        let refused = call(&id, Revision::Session, Ok(asks));
        assert_eq!(refused.status, StatusCode::BAD_GATEWAY);
        assert!(refused.headers.get(header::WWW_AUTHENTICATE).is_none());
    }
}
