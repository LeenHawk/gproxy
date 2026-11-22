//! The client against MCP servers that live in this process, one per
//! lifecycle: every exchange is a real HTTP request to an `OutboundClient`.

use std::sync::Mutex;

use gproxy_client::OutboundClient;
use gproxy_mcp::client::{self, CallError, Context, Mode, Target};
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::Bytes,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde_json::{Value, json};

const ENDPOINT: &str = "https://tools.example/mcp";

#[derive(Default)]
struct Server {
    /// Speaks 2026-07-28; otherwise only the session-based revisions.
    stateless: bool,
    /// Answers 401 with a challenge, whatever is asked.
    locked: bool,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    sessions: Vec<String>,
    /// Every request as `METHOD rpc-method`, with the headers that matter.
    seen: Vec<(String, HeaderMap, Value)>,
}

fn json_reply(status: StatusCode, body: Value) -> WireResponse<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(Bytes::from(body.to_string())),
    }
}

fn result(result: Value) -> WireResponse<HttpBody> {
    json_reply(
        StatusCode::OK,
        json!({ "jsonrpc": "2.0", "id": 1, "result": result }),
    )
}

fn empty(status: StatusCode) -> WireResponse<HttpBody> {
    WireResponse {
        status,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::new()),
    }
}

fn tool(name: &str) -> Value {
    json!({ "name": name, "inputSchema": { "type": "object" } })
}

impl Server {
    fn stateless(&self, method: &str, message: &Value) -> WireResponse<HttpBody> {
        let meta = &message["params"]["_meta"];
        assert_eq!(
            meta["io.modelcontextprotocol/protocolVersion"],
            "2026-07-28"
        );
        assert!(meta["io.modelcontextprotocol/clientCapabilities"].is_object());
        match method {
            "server/discover" => result(json!({
                "resultType": "complete",
                "supportedVersions": ["2025-11-25", "2026-07-28"],
                "capabilities": { "tools": {} },
            })),
            "tools/list" => result(json!({ "tools": [tool("search")], "nextCursor": "page-2" })),
            "tools/call" => result(json!({
                "resultType": "input_required",
                "inputRequests": { "confirm": { "method": "elicitation/create" } },
                "requestState": "opaque",
            })),
            other => panic!("unexpected method {other}"),
        }
    }

    fn session_based(
        &self,
        method: &str,
        headers: &HeaderMap,
        state: &mut State,
    ) -> WireResponse<HttpBody> {
        let session = headers
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok());
        match method {
            "initialize" => {
                let id = format!("s-{}", state.sessions.len() + 1);
                state.sessions.push(id.clone());
                let mut reply = result(json!({
                    "protocolVersion": "2025-11-25",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "fake", "version": "1" },
                }));
                reply
                    .headers
                    .insert("mcp-session-id", HeaderValue::from_str(&id).unwrap());
                reply
            }
            _ if !session.is_some_and(|id| state.sessions.iter().any(|known| known == id)) => {
                empty(StatusCode::NOT_FOUND)
            }
            "notifications/initialized" => empty(StatusCode::ACCEPTED),
            // A streamed answer: a notification first, then the response.
            "tools/list" => {
                assert_eq!(headers["mcp-protocol-version"], "2025-11-25");
                let events = format!(
                    "event: message\ndata: {}\n\nevent: message\ndata: {}\n\n",
                    json!({ "jsonrpc": "2.0", "method": "notifications/progress", "params": {} }),
                    json!({ "jsonrpc": "2.0", "id": 1, "result": { "tools": [tool("ask")] } }),
                );
                let mut reply = empty(StatusCode::OK);
                reply.headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/event-stream"),
                );
                reply.body = HttpBody::Bytes(Bytes::from(events));
                reply
            }
            "tools/call" => result(json!({ "content": [] })),
            other => panic!("unexpected method {other}"),
        }
    }
}

impl OutboundClient for Server {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            assert_eq!(parts.uri.to_string(), ENDPOINT);
            assert_eq!(parts.headers["x-fixed"], "yes");
            let HttpBody::Bytes(body) = body else {
                unreachable!("requests are buffered")
            };
            if self.locked {
                let mut reply = empty(StatusCode::UNAUTHORIZED);
                reply.headers.insert(
                    header::WWW_AUTHENTICATE,
                    HeaderValue::from_static("Bearer resource_metadata=\"https://x\""),
                );
                return Ok(reply);
            }
            let mut state = self.state.lock().unwrap();
            if parts.method == Method::DELETE {
                let id = parts.headers["mcp-session-id"].to_str().unwrap();
                state.sessions.retain(|known| known != id);
                return Ok(empty(StatusCode::OK));
            }
            let message: Value = serde_json::from_slice(&body).unwrap();
            let method = message["method"].as_str().unwrap().to_owned();
            state
                .seen
                .push((method.clone(), parts.headers.clone(), message.clone()));
            let names_stateless = parts
                .headers
                .get("mcp-protocol-version")
                .is_some_and(|version| version == "2026-07-28");
            Ok(match (self.stateless, names_stateless) {
                (true, true) => self.stateless(&method, &message),
                (false, true) => json_reply(
                    StatusCode::BAD_REQUEST,
                    json!({
                        "jsonrpc": "2.0", "id": 1,
                        "error": { "code": -32022, "message": "Unsupported protocol version" },
                    }),
                ),
                (_, false) => self.session_based(&method, &parts.headers, &mut state),
            })
        })
    }
}

fn fixed() -> HeaderMap {
    HeaderMap::from_iter([(
        header::HeaderName::from_static("x-fixed"),
        HeaderValue::from_static("yes"),
    )])
}

fn target<'a>(server: &'a Server, headers: &'a HeaderMap) -> Target<'a> {
    Target {
        client: server,
        endpoint: ENDPOINT,
        headers,
    }
}

#[tokio::test]
async fn a_stateless_server_is_used_without_a_handshake() {
    let server = Server {
        stateless: true,
        ..Server::default()
    };
    let headers = fixed();
    let target = target(&server, &headers);
    let mode = client::open(target).await.unwrap();
    assert_eq!(mode, Mode::Stateless);

    let context = Context { capabilities: None };
    let first = client::request(target, &mode, &context, "tools/list", json!({}))
        .await
        .unwrap()
        .unwrap();
    let (tools, next) = client::tools_page(&first).unwrap();
    assert_eq!(tools[0].name, "search");
    assert_eq!(next.as_deref(), Some("page-2"));

    // The caller's capabilities reach the server, and its request for input
    // comes back as the result it is.
    let capabilities = json!({ "elicitation": {} });
    let answer = client::request(
        target,
        &mode,
        &Context {
            capabilities: Some(&capabilities),
        },
        "tools/call",
        json!({ "name": "search", "arguments": { "q": "x" } }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(answer["resultType"], "input_required");
    assert_eq!(answer["requestState"], "opaque");

    let state = server.state.lock().unwrap();
    let (_, sent, message) = state.seen.last().unwrap();
    assert_eq!(sent["mcp-method"], "tools/call");
    assert_eq!(sent["mcp-name"], "search");
    assert_eq!(
        message["params"]["_meta"]["io.modelcontextprotocol/clientCapabilities"],
        capabilities
    );
    assert!(state.seen.iter().all(|(method, ..)| method != "initialize"));
}

#[tokio::test]
async fn an_older_server_is_opened_with_initialize_and_addressed_by_session() {
    let server = Server::default();
    let headers = fixed();
    let target = target(&server, &headers);
    let mode = client::open(target).await.unwrap();
    assert_eq!(
        mode,
        Mode::Session {
            id: Some("s-1".to_owned()),
            version: "2025-11-25".to_owned(),
        }
    );
    let methods: Vec<_> = server
        .state
        .lock()
        .unwrap()
        .seen
        .iter()
        .map(|(method, ..)| method.clone())
        .collect();
    assert_eq!(
        methods,
        ["server/discover", "initialize", "notifications/initialized"]
    );

    let context = Context { capabilities: None };
    let list = || client::request(target, &mode, &context, "tools/list", json!({}));
    let (tools, _) = client::tools_page(&list().await.unwrap().unwrap()).unwrap();
    assert_eq!(tools[0].name, "ask");

    // A call a stateless caller wrote: its revision and client stay behind,
    // because the session negotiated its own.
    let written_for_stateless = json!({
        "name": "ask",
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {},
            "progressToken": 3,
        },
    });
    client::request(target, &mode, &context, "tools/call", written_for_stateless)
        .await
        .unwrap()
        .unwrap();
    let sent = server.state.lock().unwrap().seen.last().unwrap().2.clone();
    assert_eq!(sent["params"]["_meta"], json!({ "progressToken": 3 }));

    client::close(target, &mode).await;
    let error = list().await.unwrap_err();
    assert!(matches!(error, CallError::SessionExpired), "{error}");
}

#[tokio::test]
async fn a_refused_credential_is_reported_without_the_servers_challenge() {
    let server = Server {
        locked: true,
        ..Server::default()
    };
    let headers = fixed();
    let error = client::open(target(&server, &headers)).await.unwrap_err();
    assert!(matches!(error, CallError::Unauthorized));
    assert_eq!(error.to_string(), "the server refused the credential");
}
