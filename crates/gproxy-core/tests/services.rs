//! Vendor service dispatch: views validated against the caller's role, one
//! usable credential (or the named one), bindings limited to the caller's
//! scope or the target's credentials, nothing observed.

mod support;

use gproxy_channel::{ChannelError, channel::CallerRole};
use gproxy_core::{
    BlockSource, CoreError, CredentialBlocks, CredentialStatus, ExecutionTarget, ServiceRequest,
    ServiceView, keys,
};
use gproxy_protocol::{HttpBody, WireRequest, connection::Bytes};
use gproxy_store::operations::credentials::CredentialStatusUpdate;
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::json;
use std::{sync::atomic::Ordering, time::Duration};
use support::{Harness, full, harness, json_reply, read};

fn service_request(method: Method, path: &str) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert("x-client-tag", HeaderValue::from_static("cli"));
    WireRequest {
        method,
        path: path.into(),
        query: None,
        headers,
        body: HttpBody::Bytes(Bytes::new()),
    }
}

fn request(
    h: &Harness,
    scope: &str,
    caller: CallerRole,
    view: ServiceView,
    method: Method,
    path: &str,
) -> ServiceRequest {
    ServiceRequest {
        scope: scope.into(),
        caller,
        view,
        target: h.target("p"),
        request: service_request(method, path),
    }
}

/// The target narrowed to one credential: the host's org boundary.
fn narrowed(h: &Harness, credential_id: &str) -> ExecutionTarget {
    let mut target = h.target("p");
    target.credentials.retain(|c| c.id == credential_id);
    target
}

#[tokio::test]
async fn the_caller_view_reaches_the_channel_with_the_selected_credential() {
    let h = harness(full(), "round_robin").await;
    h.channel.expose_services.store(true, Ordering::Relaxed);
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"account": {"email": "user@example.com"}}),
    )]);
    let response = h
        .core
        .call_service(request(
            &h,
            "tenant",
            CallerRole::Member,
            ServiceView::Caller,
            Method::GET,
            "/api/oauth/profile",
        ))
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        read(response.body).await,
        json!({"account": {"email": "user@example.com"}}).to_string(),
        "the channel's answer is returned as-is"
    );
    assert_eq!(
        h.channel.service_calls.lock().unwrap().as_slice(),
        ["a:Member:tenant"],
        "the first eligible credential, the role and the scope as identity"
    );
    let seen = h.client.seen.lines();
    assert!(
        seen[0].starts_with("GET https://up.example/api/oauth/profile auth=Bearer ka"),
        "{}",
        seen[0]
    );
    assert!(
        h.observer.reports.lock().unwrap().is_empty() && h.observer.log.lines().is_empty(),
        "services run outside the funnel: no usage, no capture, no trace"
    );
}

#[tokio::test]
async fn admin_only_views_are_validated_against_the_role_and_the_target() {
    let h = harness(full(), "round_robin").await;
    h.channel.expose_services.store(true, Ordering::Relaxed);
    for view in [ServiceView::Pool, ServiceView::Credential("b".into())] {
        let error = h
            .core
            .call_service(request(
                &h,
                "tenant",
                CallerRole::Member,
                view,
                Method::GET,
                "/api/x",
            ))
            .await
            .unwrap_err();
        assert!(matches!(error, CoreError::Forbidden(_)), "{error}");
    }
    let error = h
        .core
        .call_service(request(
            &h,
            "tenant",
            CallerRole::Admin,
            ServiceView::Credential("zzz".into()),
            Method::GET,
            "/api/x",
        ))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::InvalidTarget(_)), "{error}");
    assert!(h.client.seen.lines().is_empty());

    h.script(vec![
        json_reply(StatusCode::OK, json!({})),
        json_reply(StatusCode::OK, json!({})),
    ]);
    h.core
        .call_service(request(
            &h,
            "tenant",
            CallerRole::Admin,
            ServiceView::Credential("b".into()),
            Method::GET,
            "/api/x",
        ))
        .await
        .unwrap();
    h.core
        .call_service(request(
            &h,
            "tenant",
            CallerRole::Admin,
            ServiceView::Pool,
            Method::GET,
            "/api/x",
        ))
        .await
        .unwrap();
    assert_eq!(
        h.channel.service_calls.lock().unwrap().as_slice(),
        ["b:Admin:p:a,b", "a:Admin:p:a,b"],
        "the named credential, then the pool identity over the target's credential set"
    );
    assert!(h.client.seen.lines()[0].contains("auth=Bearer kb"));
}

#[tokio::test]
async fn a_channel_without_services_is_unsupported() {
    let h = harness(full(), "round_robin").await;
    let error = h
        .core
        .call_service(request(
            &h,
            "tenant",
            CallerRole::Admin,
            ServiceView::Caller,
            Method::GET,
            "/api/oauth/profile",
        ))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CoreError::Channel(ChannelError::UnsupportedService)
    ));
    assert!(h.client.seen.lines().is_empty());
}

#[tokio::test]
async fn blocked_and_dead_credentials_are_skipped() {
    let h = harness(full(), "round_robin").await;
    h.channel.expose_services.store(true, Ordering::Relaxed);
    let mut blocks = CredentialBlocks::default();
    blocks.upsert(
        gproxy_core::CredentialBlock {
            scope: gproxy_channel::channel::QuotaScope::All,
            operation: None,
            until_ms: i64::MAX,
            source: BlockSource::RateLimited,
            observed_at_ms: 0,
        },
        0,
    );
    h.core
        .cache()
        .put(
            &keys::credential_blocks("p", "a"),
            serde_json::to_vec(&blocks).unwrap(),
            Duration::from_secs(3600),
        )
        .await
        .unwrap();
    h.script(vec![json_reply(StatusCode::OK, json!({}))]);
    h.core
        .call_service(request(
            &h,
            "tenant",
            CallerRole::Member,
            ServiceView::Caller,
            Method::GET,
            "/api/oauth/usage",
        ))
        .await
        .unwrap();
    assert_eq!(
        h.channel.service_calls.lock().unwrap().as_slice(),
        ["b:Member:tenant"]
    );

    h.core
        .store()
        .credentials()
        .set_status_many(vec![CredentialStatusUpdate {
            id: "b".into(),
            expected_version: 0,
            status: CredentialStatus::Dead,
            reason: Some("invalid_grant".into()),
        }])
        .await
        .unwrap();
    h.core.reload_credentials(&["b".into()]).await.unwrap();
    let error = h
        .core
        .call_service(request(
            &h,
            "tenant",
            CallerRole::Member,
            ServiceView::Caller,
            Method::GET,
            "/api/oauth/usage",
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(
            &error,
            CoreError::CredentialDead { credential_id, reason }
                if credential_id == "b" && reason.as_deref() == Some("invalid_grant")
        ),
        "{error}"
    );
    let error = h
        .core
        .call_service(request(
            &h,
            "tenant",
            CallerRole::Admin,
            ServiceView::Credential("b".into()),
            Method::GET,
            "/api/oauth/usage",
        ))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::CredentialDead { .. }));
    assert_eq!(h.client.seen.lines().len(), 1, "no further upstream call");
}

#[tokio::test]
async fn bindings_are_scoped_to_the_caller_or_to_the_targets_credentials() {
    let h = harness(full(), "round_robin").await;
    h.channel.expose_services.store(true, Ordering::Relaxed);
    let bind = |scope: &str, caller, view, id: &str| {
        h.core.call_service(request(
            &h,
            scope,
            caller,
            view,
            Method::POST,
            &format!("/bind/thing/{id}"),
        ))
    };
    bind("t1", CallerRole::Member, ServiceView::Caller, "x1")
        .await
        .unwrap();
    bind("t2", CallerRole::Member, ServiceView::Caller, "x2")
        .await
        .unwrap();
    bind(
        "t1",
        CallerRole::Admin,
        ServiceView::Credential("b".into()),
        "x3",
    )
    .await
    .unwrap();
    let list = |scope: String, caller, view, target: ExecutionTarget| {
        let core = &h.core;
        async move {
            let response = core
                .call_service(ServiceRequest {
                    scope,
                    caller,
                    view,
                    target,
                    request: service_request(Method::GET, "/bindings/thing"),
                })
                .await
                .unwrap();
            serde_json::from_str::<Vec<String>>(&read(response.body).await).unwrap()
        }
    };
    assert_eq!(
        list(
            "t1".into(),
            CallerRole::Member,
            ServiceView::Caller,
            h.target("p")
        )
        .await,
        ["x1@a"],
        "the caller sees its own scope only, not what it created under the credential view"
    );
    assert_eq!(
        list(
            "t2".into(),
            CallerRole::Member,
            ServiceView::Caller,
            h.target("p")
        )
        .await,
        ["x2@a"]
    );
    let mut pool = list(
        "t9".into(),
        CallerRole::Admin,
        ServiceView::Pool,
        h.target("p"),
    )
    .await;
    pool.sort();
    assert_eq!(
        pool,
        ["x1@a", "x2@a", "x3@b"],
        "the pool sees every binding of its credentials"
    );
    assert_eq!(
        list(
            "t9".into(),
            CallerRole::Admin,
            ServiceView::Pool,
            narrowed(&h, "b")
        )
        .await,
        ["x3@b"],
        "a target narrowed by the host narrows the pool"
    );
    let rows = h
        .core
        .store()
        .resource_bindings()
        .query(sea_orm::EntityTrait::find())
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .all(|r| r.provider_id == "p" && r.generation == 0)
    );
}
