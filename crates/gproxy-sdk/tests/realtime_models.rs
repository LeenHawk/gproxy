#![cfg(not(target_arch = "wasm32"))]
mod support;
use futures_util::StreamExt;
use gproxy_protocol::{
    Dialect, Operation, OperationKey, WireRequest, capability::UpstreamConnection,
};
use support::seed::{self, WsReply};

#[tokio::test]
async fn a_query_only_realtime_selector_resolves_an_alias() {
    let (sdk, client, _) = seed::handle().await;
    seed::provider(&sdk, "p", "alt", &["m1"]).await;
    seed::credential(&sdk, "c", "p").await;
    seed::route(
        &sdk,
        "r",
        "voice-alias",
        gproxy_store::entity::routing::route::RouteStrategy::Weighted,
        1,
        &[("member", "p", "m1", 0, 100)],
    )
    .await;
    seed::publish(&sdk).await;
    client.script_ws(vec![WsReply::Connected]);
    let execution = sdk
        .connect(
            OperationKey {
                operation: Operation::ConnectRealtime,
                dialect: Dialect::OpenAi,
            },
            WireRequest {
                method: http::Method::GET,
                path: "/v1/realtime".into(),
                query: Some("model=voice-alias".into()),
                headers: http::HeaderMap::new(),
                body: (),
            },
        )
        .scope("tenant")
        .send()
        .await
        .unwrap();
    let (connection, completion) = execution.into_parts();
    let UpstreamConnection::Connected { mut socket, .. } = connection else {
        panic!("connected")
    };
    while socket.incoming.next().await.is_some() {}
    drop(socket);
    completion.await.unwrap();
    assert_eq!(client.urls().len(), 1);
}
