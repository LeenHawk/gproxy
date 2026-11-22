//! The MCP gateway through the router: one registered server, a caller with
//! nothing but a gateway key, and the upstream scripted request by request.

mod support;

use gproxy_app::dto::{AuditQuery, McpScopeWrite, McpToolNatureWrite};
use gproxy_core::PlaintextCodec;
use gproxy_sdk::SecretCodec;
use gproxy_store::entity::upstream::{credential, provider};
use http::StatusCode;
use sea_orm::Set;
use serde_json::{Value, json};
use support::{Answer, Host, Reply, keyed, post};

fn rpc(result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "result": result })
}

/// What a server that only speaks the session-based revisions answers while
/// the gateway opens it: no to `server/discover`, a session to `initialize`.
fn opening() -> Vec<Reply> {
    vec![
        Reply::Http(
            StatusCode::BAD_REQUEST,
            json!({
                "jsonrpc": "2.0", "id": 1,
                "error": { "code": -32022, "message": "Unsupported protocol version" },
            }),
        ),
        Reply::Headed(
            StatusCode::OK,
            vec![("mcp-session-id", "upstream-1".to_owned())],
            rpc(json!({
                "protocolVersion": "2025-11-25",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "wiki", "version": "1" },
            })),
        ),
        Reply::Http(StatusCode::ACCEPTED, Value::Null),
    ]
}

fn tool_list() -> Reply {
    Reply::Http(
        StatusCode::OK,
        rpc(json!({ "tools": [
            {
                "name": "ask",
                "inputSchema": { "type": "object" },
                "annotations": { "readOnlyHint": true },
            },
            { "name": "edit", "inputSchema": { "type": "object" } },
        ] })),
    )
}

/// One MCP server `wiki` with a shared pasted secret, and `alice` with a key.
async fn instance() -> Host {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    handle
        .store()
        .providers()
        .create_many(vec![provider::ActiveModel {
            id: Set("p-wiki".into()),
            name: Set("wiki".into()),
            channel: Set("mcp".into()),
            base_url: Set(Some("https://wiki.example/mcp".into())),
            config: Set(json!({ "headers": { "x-tenant": "acme" } })),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    handle
        .store()
        .credentials()
        .create_many(vec![credential::ActiveModel {
            id: Set("c-wiki".into()),
            provider_id: Set("p-wiki".into()),
            auth_kind: Set("api_key".into()),
            secret: Set(PlaintextCodec
                .seal("c-wiki", &json!({ "api_key": "upstream-secret" }))
                .unwrap()),
            metadata: Set(json!({})),
            enabled: Set(true),
            ..Default::default()
        }])
        .await
        .unwrap();
    host.publish().await;
    host
}

async fn mcp(host: &Host, path: &str, method: &str, params: Value) -> Answer {
    let body = json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params });
    host.send(keyed(post(path, body), "k-alice")).await
}

fn names(answer: &Answer) -> Vec<String> {
    answer.json()["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn tools_are_listed_called_narrowed_by_scopes_and_audited() {
    let host = instance().await;
    let mut script = opening();
    script.push(tool_list());
    host.client.script(script);

    // The server's own endpoint: its tools under their own names.
    let listed = mcp(&host, "/connect/mcp/wiki", "tools/list", json!({})).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    assert_eq!(names(&listed), ["ask", "edit"]);
    // What the gateway settled on is what the caller is told: an unmarked
    // tool is destructive.
    let edit = &listed.json()["result"]["tools"][1]["annotations"];
    assert_eq!(edit["destructiveHint"], true);
    assert_eq!(edit["readOnlyHint"], false);

    // The aggregated endpoint: the same session and the cached list, so the
    // server is not asked again.
    let aggregated = mcp(&host, "/connect/mcp", "tools/list", json!({})).await;
    assert_eq!(names(&aggregated), ["wiki__ask", "wiki__edit"]);
    assert_eq!(host.client.urls().len(), 4);

    host.client.push(Reply::Http(
        StatusCode::OK,
        rpc(json!({ "content": [{ "type": "text", "text": "an answer" }] })),
    ));
    let called = mcp(
        &host,
        "/connect/mcp",
        "tools/call",
        json!({ "name": "wiki__ask", "arguments": { "question": "why" } }),
    )
    .await;
    assert_eq!(called.json()["id"], 7);
    assert_eq!(
        called.json()["result"]["content"][0]["text"],
        "an answer",
        "{}",
        called.text()
    );
    let sent = host.client.bodies().pop().unwrap();
    assert_eq!(sent["method"], "tools/call");
    assert_eq!(sent["params"]["name"], "ask");

    // One rule for alice: only what reads. `edit` disappears, and calling it
    // is answered without asking the server.
    let data = host.data();
    host.operations(&data)
        .mcp_scopes()
        .create(McpScopeWrite {
            user_id: Some("alice".into()),
            scope: "mcp:wiki:@read".into(),
            action: "allow".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    host.publish().await;
    let narrowed = mcp(&host, "/connect/mcp", "tools/list", json!({})).await;
    assert_eq!(names(&narrowed), ["wiki__ask"]);
    let refused = mcp(
        &host,
        "/connect/mcp",
        "tools/call",
        json!({ "name": "wiki__edit", "arguments": {} }),
    )
    .await;
    assert_eq!(
        refused.json()["error"]["message"],
        "Unknown tool: wiki__edit"
    );
    assert_eq!(host.client.urls().len(), 5, "the server was not asked");

    // The administrator says `edit` only reads: it is back.
    let data = host.data();
    host.operations(&data)
        .mcp_tool_natures()
        .create(McpToolNatureWrite {
            provider_id: "p-wiki".into(),
            tool: "edit".into(),
            nature: "read".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    host.publish().await;
    let restored = mcp(&host, "/connect/mcp", "tools/list", json!({})).await;
    assert_eq!(names(&restored), ["wiki__ask", "wiki__edit"]);

    let data = host.data();
    let trail = host
        .operations(&data)
        .audit()
        .query(AuditQuery {
            action: Some("mcp.tools.call".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let rows: Vec<_> = trail
        .items
        .iter()
        .map(|row| {
            (
                row.outcome.as_str(),
                row.detail["tool"].as_str().unwrap(),
                row.detail["outcome"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows.contains(&("ok", "ask", "ok")));
    assert!(rows.contains(&("error", "edit", "refused")));
    let ask = trail
        .items
        .iter()
        .find(|row| row.detail["tool"] == "ask")
        .unwrap();
    assert_eq!(ask.actor_user_id.as_deref(), Some("alice"));
    assert_eq!(ask.actor_api_key_id.as_deref(), Some("k-alice"));
    assert_eq!(ask.entity_id.as_deref(), Some("p-wiki"));
    assert_eq!(ask.detail["server"], "wiki");
    assert_eq!(ask.detail["credentialId"], "c-wiki");
    // Payloads are not kept unless the instance logs bodies.
    assert!(ask.detail.get("arguments").is_none());
}

#[tokio::test]
async fn a_server_that_refuses_the_credential_is_a_plain_502_without_its_challenge() {
    let host = instance().await;
    host.client.script(vec![Reply::Headed(
        StatusCode::UNAUTHORIZED,
        vec![(
            "www-authenticate",
            "Bearer resource_metadata=\"https://wiki.example/.well-known/oauth-protected-resource\""
                .to_owned(),
        )],
        json!({}),
    )]);
    let answer = mcp(&host, "/connect/mcp/wiki", "tools/list", json!({})).await;
    assert_eq!(answer.status, StatusCode::BAD_GATEWAY);
    assert!(answer.header("www-authenticate").is_none());
    assert_eq!(answer.json()["id"], 7);
    assert!(
        answer.json()["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("MCP server `wiki`"),
        "{}",
        answer.text()
    );
}

#[tokio::test]
async fn a_caller_without_a_credential_for_the_server_does_not_have_it() {
    let host = instance().await;
    let handle = host.handle();
    support::person(&handle, "bob", "user").await;
    support::api_key(&handle, "k-bob", "bob", None, None).await;
    // The only credential becomes alice's own.
    use sea_orm::{ActiveModelTrait, IntoActiveModel};
    let mut row = handle
        .store()
        .credentials()
        .get_many(&["c-wiki".to_owned()])
        .await
        .unwrap()
        .remove(0)
        .unwrap()
        .into_active_model();
    row.user_id = Set(Some("alice".into()));
    row.update(handle.store().connection()).await.unwrap();
    host.publish().await;

    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
    let one = host
        .send(keyed(post("/connect/mcp/wiki", body.clone()), "k-bob"))
        .await;
    assert_eq!(one.status, StatusCode::NOT_FOUND);
    let all = host.send(keyed(post("/connect/mcp", body), "k-bob")).await;
    assert_eq!(all.json()["result"]["tools"], json!([]));
    assert!(host.client.urls().is_empty());
}
