#![cfg(not(target_arch = "wasm32"))]
mod support;
use futures_util::{SinkExt, StreamExt};
use gproxy_app::{AppError, DataPlaneRequest};
use gproxy_protocol::{
    Dialect, Operation, OperationKey,
    connection::{Bytes, WsFrame},
};
use gproxy_store::entity::identity::permission;
use sea_orm::{EntityTrait, Set};

async fn instance() -> (
    std::sync::Arc<gproxy_app::App<sea_orm::DatabaseConnection>>,
    std::sync::Arc<support::ScriptClient>,
) {
    let (app, client) = support::app().await;
    let handle = app.gproxy();
    support::person(handle, "alice", "user").await;
    support::api_key(handle, "k-alice", "alice", None, None).await;
    support::provider(handle, "p1", &["m1", "m2"]).await;
    support::credential(handle, "c", "p1", None, None, None).await;
    support::allow(handle, "only-model", "alice", None).await;
    permission::Entity::update(permission::ActiveModel {
        id: Set("only-model".into()),
        user_id: Set(None),
        api_key_id: Set(Some("k-alice".into())),
        model_pattern: Set("p1/m1".into()),
        ..Default::default()
    })
    .exec(handle.store().connection())
    .await
    .unwrap();
    support::publish(&app).await;
    (std::sync::Arc::new(app), client)
}
fn request(dialect: Dialect, query: &str) -> DataPlaneRequest {
    let (parts, ()) = http::Request::builder()
        .uri(format!("/v1/realtime{query}"))
        .body(())
        .unwrap()
        .into_parts();
    DataPlaneRequest::new(
        "realtime",
        OperationKey {
            operation: Operation::ConnectRealtime,
            dialect,
        },
        parts,
        Bytes::new(),
    )
}
#[tokio::test]
async fn query_model_is_admitted_before_opening_the_upstream() {
    let (app, client) = instance().await;
    let caller = support::caller_for(&app, "k-alice").await;
    assert!(matches!(
        app.connect(&caller, request(Dialect::OpenAi, "?model=p1%2Fm2"))
            .await,
        Err(AppError::Forbidden(_))
    ));
    for query in ["", "?model=", "?model=p1/m1&model=p1/m2"] {
        assert_eq!(
            app.connect(&caller, request(Dialect::OpenAi, query))
                .await
                .unwrap_err()
                .status_code(),
            400
        );
    }
    assert!(client.urls().is_empty());
}
#[tokio::test]
async fn gemini_cannot_connect_until_setup_selects_a_permitted_model() {
    let (app, client) = instance().await;
    let caller = support::caller_for(&app, "k-alice").await;
    assert_eq!(
        app.connect(&caller, request(Dialect::Gemini, ""))
            .await
            .unwrap_err()
            .status_code(),
        400
    );
    let mut socket = app
        .open_gemini_realtime(caller, request(Dialect::Gemini, ""), None)
        .unwrap();
    assert!(client.urls().is_empty());
    socket
        .outgoing
        .send(WsFrame::Text(
            r#"{"setup":{"model":"models/p1/m2"}}"#.into(),
        ))
        .await
        .unwrap();
    let WsFrame::Text(error) = socket.incoming.next().await.unwrap().unwrap() else {
        panic!("error")
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&error).unwrap()["error"]["code"],
        403
    );
    assert!(client.urls().is_empty());
}
