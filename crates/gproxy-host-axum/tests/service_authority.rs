#![cfg(not(target_arch = "wasm32"))]

mod support;

use gproxy_store::entity::identity::api_key;
use http::StatusCode;
use sea_orm::{EntityTrait, Set};
use support::{Host, get, keyed, with};

#[tokio::test]
async fn ordinary_admin_key_cannot_request_privileged_vendor_views() {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "root", "admin").await;
    support::api_key(&handle, "model-key", "root", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    api_key::Entity::update(api_key::ActiveModel {
        id: Set("model-key".into()),
        management: Set(false),
        ..Default::default()
    })
    .exec(handle.store().connection())
    .await
    .unwrap();
    host.publish().await;

    for view in ["pool", "credential:c1"] {
        let answer = host
            .send(with(
                keyed(get("/p1/backend-api/wham/usage"), "model-key"),
                "x-gproxy-view",
                view,
            ))
            .await;
        assert_eq!(answer.status, StatusCode::FORBIDDEN, "{}", answer.text());
    }
    let answer = host
        .send(keyed(get("/p1/backend-api/wham/usage"), "model-key"))
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.json()["role"], "member");
    assert_eq!(answer.json()["view"], "caller");
}
