//! Resolution: what a model name means, who is allowed to serve it, and in
//! which order the candidates are tried.

mod support;

use std::collections::BTreeSet;

use gproxy_protocol::{Dialect, Operation, OperationKey};
use gproxy_sdk::{ResolveRequest, SdkError};
use gproxy_store::entity::routing::route::RouteStrategy;
use support::seed::{self, Handle};

fn generate() -> OperationKey {
    OperationKey {
        operation: Operation::GenerateContent,
        dialect: Dialect::OpenAi,
    }
}

fn request(model: &str) -> ResolveRequest<'_> {
    ResolveRequest::new(generate()).model(model)
}

/// The provider ids of the plan, in plan order.
async fn order(gproxy: &Handle, model: &str) -> Vec<String> {
    gproxy
        .resolve(request(model))
        .await
        .unwrap()
        .targets
        .iter()
        .map(|target| target.provider.entity.id.clone())
        .collect()
}

/// Three providers of the `test` channel, one credential each.
async fn three(gproxy: &Handle) {
    for id in ["p1", "p2", "p3"] {
        seed::provider(gproxy, id, "test", &["m1"]).await;
        seed::credential(gproxy, &format!("c-{id}"), id).await;
    }
}

#[tokio::test]
async fn a_route_prefers_the_lower_tier_over_the_higher_weight() {
    let (gproxy, _, _) = seed::handle().await;
    three(&gproxy).await;
    seed::route(
        &gproxy,
        "r",
        "fast",
        RouteStrategy::Failover,
        4,
        &[
            ("m-p1", "p1", "m1", 0, 50),
            ("m-p2", "p2", "m1", 0, 100),
            // A far heavier member, but in the fallback tier.
            ("m-p3", "p3", "m1", 1, 900),
        ],
    )
    .await;
    seed::publish(&gproxy).await;

    let plan = gproxy.resolve(request("fast")).await.unwrap();
    assert_eq!(
        plan.targets
            .iter()
            .map(|t| t.provider.entity.id.as_str())
            .collect::<Vec<_>>(),
        ["p2", "p1", "p3"],
        "weight orders within a tier; a later tier is never balanced against an earlier one"
    );
    assert_eq!(plan.max_attempts.get(), 4, "the route's own attempt budget");
    assert_eq!(plan.resolved_model.as_deref(), Some("fast"));
    assert_eq!(plan.targets[0].upstream_model.as_deref(), Some("m1"));
    assert_eq!(plan.targets[0].member_id.as_deref(), Some("m-p2"));
}

#[tokio::test]
async fn failover_keeps_the_same_order_every_time() {
    let (gproxy, _, _) = seed::handle().await;
    three(&gproxy).await;
    seed::route(
        &gproxy,
        "r",
        "fixed",
        RouteStrategy::Failover,
        6,
        &[
            ("m-a", "p1", "m1", 0, 100),
            ("m-b", "p2", "m1", 0, 100),
            ("m-c", "p3", "m1", 0, 100),
        ],
    )
    .await;
    seed::publish(&gproxy).await;

    for _ in 0..3 {
        assert_eq!(order(&gproxy, "fixed").await, ["p1", "p2", "p3"]);
    }
}

#[tokio::test]
async fn round_robin_rotates_the_leading_run() {
    let (gproxy, _, _) = seed::handle().await;
    three(&gproxy).await;
    seed::route(
        &gproxy,
        "r",
        "spread",
        RouteStrategy::RoundRobin,
        6,
        &[
            ("m-a", "p1", "m1", 0, 100),
            ("m-b", "p2", "m1", 0, 100),
            ("m-c", "p3", "m1", 0, 100),
        ],
    )
    .await;
    seed::publish(&gproxy).await;

    let mut heads = Vec::new();
    for _ in 0..4 {
        heads.push(order(&gproxy, "spread").await[0].clone());
    }
    assert_eq!(
        heads,
        ["p1", "p2", "p3", "p1"],
        "the counter advances once per resolution and wraps the run"
    );
    // Rotation, not reshuffling: the run keeps its relative order.
    assert_eq!(order(&gproxy, "spread").await, ["p2", "p3", "p1"]);
}

#[tokio::test]
async fn weighted_follows_the_smooth_sequence() {
    let (gproxy, _, _) = seed::handle().await;
    three(&gproxy).await;
    seed::route(
        &gproxy,
        "r",
        "shared",
        RouteStrategy::Weighted,
        6,
        &[("m-a", "p1", "m1", 0, 3), ("m-b", "p2", "m1", 0, 1)],
    )
    .await;
    seed::publish(&gproxy).await;

    let mut heads = Vec::new();
    for _ in 0..4 {
        heads.push(order(&gproxy, "shared").await[0].clone());
    }
    assert_eq!(
        heads,
        ["p1", "p1", "p2", "p1"],
        "3:1 spread over the cycle rather than three of one and then the other"
    );
}

#[tokio::test]
async fn a_fully_blocked_provider_is_ordered_last_but_kept() {
    let (gproxy, _, _) = seed::handle().await;
    three(&gproxy).await;
    seed::route(
        &gproxy,
        "r",
        "health",
        RouteStrategy::Failover,
        6,
        &[("m-a", "p1", "m1", 0, 100), ("m-b", "p2", "m1", 0, 100)],
    )
    .await;
    seed::publish(&gproxy).await;
    assert_eq!(order(&gproxy, "health").await, ["p1", "p2"]);

    // p1 sorts first by id, so only health can move it.
    seed::block(&gproxy, "p1", "c-p1").await;
    let plan = gproxy.resolve(request("health")).await.unwrap();
    assert_eq!(
        plan.targets
            .iter()
            .map(|t| t.provider.entity.id.as_str())
            .collect::<Vec<_>>(),
        ["p2", "p1"],
        "a rate-limited provider is a last resort, not an outage"
    );
    assert_eq!(
        plan.targets[1].credentials.len(),
        1,
        "its blocked credential is still offered, because nothing else is left there"
    );
}

#[tokio::test]
async fn a_blocked_credential_is_dropped_while_a_healthy_one_remains() {
    let (gproxy, _, _) = seed::handle().await;
    seed::provider(&gproxy, "p1", "test", &["m1"]).await;
    seed::credential(&gproxy, "c-a", "p1").await;
    seed::credential(&gproxy, "c-b", "p1").await;
    seed::publish(&gproxy).await;

    seed::block(&gproxy, "p1", "c-a").await;
    let plan = gproxy.resolve(request("test/m1")).await.unwrap();
    assert_eq!(
        plan.targets[0]
            .credentials
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["c-b"]
    );
}

#[tokio::test]
async fn dead_and_disabled_credentials_never_appear() {
    let (gproxy, _, _) = seed::handle().await;
    seed::provider(&gproxy, "p1", "test", &["m1"]).await;
    seed::credential(&gproxy, "c-ok", "p1").await;
    seed::dead_credential(&gproxy, "c-dead", "p1").await;
    seed::disabled_credential(&gproxy, "c-off", "p1").await;
    seed::publish(&gproxy).await;

    let plan = gproxy.resolve(request("test/m1")).await.unwrap();
    assert_eq!(
        plan.targets[0]
            .credentials
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["c-ok"],
        "waiting does not revive either of the other two"
    );
}

#[tokio::test]
async fn a_provider_without_a_usable_credential_is_not_a_target() {
    let (gproxy, _, _) = seed::handle().await;
    seed::provider(&gproxy, "p1", "test", &["m1"]).await;
    seed::dead_credential(&gproxy, "c-dead", "p1").await;
    seed::publish(&gproxy).await;

    let error = gproxy
        .resolve(request("test/m1"))
        .await
        .expect_err("nothing is left to send to");
    assert!(matches!(error, SdkError::NoTarget(name) if name == "test/m1"));
}

#[tokio::test]
async fn a_channel_prefix_selects_that_channel_and_prefers_its_catalog() {
    let (gproxy, _, _) = seed::handle().await;
    // Two providers on `test`, only one of which lists the model.
    seed::provider(&gproxy, "p1", "test", &["m1"]).await;
    seed::provider(&gproxy, "p2", "test", &["other"]).await;
    seed::provider(&gproxy, "p3", "alt", &["m1"]).await;
    for (credential, provider) in [("c1", "p1"), ("c2", "p2"), ("c3", "p3")] {
        seed::credential(&gproxy, credential, provider).await;
    }
    seed::publish(&gproxy).await;

    let plan = gproxy.resolve(request("test/m1")).await.unwrap();
    assert_eq!(plan.provider_ids(), ["p1"], "the catalog decides");
    assert_eq!(plan.resolved_model.as_deref(), Some("m1"));
    assert_eq!(plan.targets[0].upstream_model.as_deref(), Some("m1"));
    assert_eq!(plan.targets[0].member_id, None);

    // No provider of the channel lists this one, so the catalog says nothing
    // and every provider of the channel is a candidate.
    let mut ids = gproxy
        .resolve(request("test/unlisted"))
        .await
        .unwrap()
        .provider_ids()
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(ids, ["p1", "p2"]);
}

#[tokio::test]
async fn a_channel_id_wins_over_a_provider_of_the_same_name() {
    let (gproxy, _, _) = seed::handle().await;
    // A provider literally named `test`, which is also a channel id.
    seed::provider_named(
        &gproxy,
        "impostor",
        "test",
        "alt",
        &["m1"],
        serde_json::json!({}),
    )
    .await;
    seed::provider(&gproxy, "real", "test", &["m1"]).await;
    seed::credential(&gproxy, "c1", "impostor").await;
    seed::credential(&gproxy, "c2", "real").await;
    seed::publish(&gproxy).await;

    assert_eq!(
        gproxy
            .resolve(request("test/m1"))
            .await
            .unwrap()
            .provider_ids(),
        ["real"],
        "a channel id cannot be renamed out of the way, so it wins"
    );
    // The provider is still reachable under its own channel prefix.
    assert_eq!(
        gproxy
            .resolve(request("alt/m1"))
            .await
            .unwrap()
            .provider_ids(),
        ["impostor"]
    );
}

#[tokio::test]
async fn a_provider_name_prefix_selects_one_provider() {
    let (gproxy, _, _) = seed::handle().await;
    seed::provider_named(
        &gproxy,
        "p1",
        "openai-main",
        "test",
        &[],
        serde_json::json!({}),
    )
    .await;
    seed::provider(&gproxy, "p2", "test", &[]).await;
    seed::credential(&gproxy, "c1", "p1").await;
    seed::credential(&gproxy, "c2", "p2").await;
    seed::publish(&gproxy).await;

    let plan = gproxy.resolve(request("openai-main/gpt-5")).await.unwrap();
    assert_eq!(plan.provider_ids(), ["p1"]);
    assert_eq!(plan.targets[0].upstream_model.as_deref(), Some("gpt-5"));
    assert_eq!(plan.resolved_model.as_deref(), Some("gpt-5"));
}

#[tokio::test]
async fn an_exposed_name_is_matched_before_any_prefix() {
    let (gproxy, _, _) = seed::handle().await;
    seed::provider(&gproxy, "p1", "test", &[]).await;
    seed::provider(&gproxy, "p2", "alt", &[]).await;
    seed::credential(&gproxy, "c1", "p1").await;
    seed::credential(&gproxy, "c2", "p2").await;
    // An exposed name that looks exactly like a channel prefix.
    seed::route(
        &gproxy,
        "r",
        "test/m1",
        RouteStrategy::Failover,
        6,
        &[("m-a", "p2", "upstream-name", 0, 100)],
    )
    .await;
    seed::publish(&gproxy).await;

    let plan = gproxy.resolve(request("test/m1")).await.unwrap();
    assert_eq!(plan.provider_ids(), ["p2"]);
    assert_eq!(
        plan.targets[0].upstream_model.as_deref(),
        Some("upstream-name"),
        "the route's member decides the upstream model, not the exposed name"
    );
}

#[tokio::test]
async fn an_unrecognized_name_and_an_unusable_one_are_different_errors() {
    let (gproxy, _, _) = seed::handle().await;
    seed::provider(&gproxy, "p1", "test", &["m1"]).await;
    seed::credential(&gproxy, "c1", "p1").await;
    seed::publish(&gproxy).await;

    // Neither an exposed name, nor a channel, nor a provider.
    for name in ["gpt-5", "nosuch/m1", "/m1", "test/"] {
        let error = gproxy
            .resolve(request(name))
            .await
            .expect_err("the name means nothing here");
        assert!(
            matches!(error, SdkError::UnknownModel(_)),
            "{name}: {error}"
        );
        assert_eq!(error.status_code(), 404);
    }

    // The name is understood; nothing behind it is reachable.
    let error = gproxy
        .resolve(ResolveRequest {
            allowed_providers: Some(&BTreeSet::new()),
            ..request("test/m1")
        })
        .await
        .expect_err("an empty allowance reaches nothing");
    assert!(matches!(error, SdkError::NoTarget(_)), "{error}");
    assert_eq!(error.status_code(), 503);
}

#[tokio::test]
async fn narrowing_intersects_providers_credentials_and_channel() {
    let (gproxy, _, _) = seed::handle().await;
    seed::provider(&gproxy, "p1", "test", &["m1"]).await;
    seed::provider(&gproxy, "p2", "alt", &["m1"]).await;
    seed::credential(&gproxy, "c1a", "p1").await;
    seed::credential(&gproxy, "c1b", "p1").await;
    seed::credential(&gproxy, "c2", "p2").await;
    seed::publish(&gproxy).await;

    // Every provider, because no model was named.
    let all = ResolveRequest::new(generate());
    let mut ids = gproxy
        .resolve(all)
        .await
        .unwrap()
        .provider_ids()
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(ids, ["p1", "p2"]);

    let channel = "alt";
    assert_eq!(
        gproxy
            .resolve(ResolveRequest {
                channel: Some(channel),
                ..all
            })
            .await
            .unwrap()
            .provider_ids(),
        ["p2"]
    );

    let providers: BTreeSet<String> = ["p1".to_owned()].into();
    assert_eq!(
        gproxy
            .resolve(ResolveRequest {
                allowed_providers: Some(&providers),
                ..all
            })
            .await
            .unwrap()
            .provider_ids(),
        ["p1"]
    );

    // A credential allowance narrows within the provider as well as across
    // providers: p2's only credential is not in the set, so p2 disappears.
    let credentials: BTreeSet<String> = ["c1b".to_owned()].into();
    let plan = gproxy
        .resolve(ResolveRequest {
            allowed_credentials: Some(&credentials),
            ..all
        })
        .await
        .unwrap();
    assert_eq!(plan.provider_ids(), ["p1"]);
    assert_eq!(
        plan.targets[0]
            .credentials
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["c1b"]
    );
}

#[tokio::test]
async fn a_sticky_provider_offers_a_session_the_same_credential() {
    let (gproxy, _, _) = seed::handle().await;
    seed::provider_named(
        &gproxy,
        "p1",
        "p1",
        "test",
        &["m1"],
        serde_json::json!({"credential_strategy": "sticky"}),
    )
    .await;
    for id in ["c-a", "c-b", "c-c", "c-d"] {
        seed::credential(&gproxy, id, "p1").await;
    }
    seed::publish(&gproxy).await;

    let head = async |key: &str| {
        gproxy
            .resolve(ResolveRequest {
                affinity_key: Some(key),
                ..request("test/m1")
            })
            .await
            .unwrap()
            .targets[0]
            .credentials[0]
            .id
            .clone()
    };

    let pinned = head("session-a").await;
    assert_eq!(
        pinned,
        head("session-a").await,
        "the same session is offered the same credential every time"
    );

    let mut seen = BTreeSet::new();
    for key in ["session-a", "session-b", "session-c", "session-d"] {
        seen.insert(head(key).await);
    }
    assert!(seen.len() > 1, "sessions spread over the credential set");
}

#[tokio::test]
async fn a_disabled_route_member_and_a_disabled_exposure_are_not_candidates() {
    let (gproxy, _, _) = seed::handle().await;
    three(&gproxy).await;
    seed::route(
        &gproxy,
        "r",
        "named",
        RouteStrategy::Failover,
        6,
        &[("m-a", "p1", "m1", 0, 100), ("m-b", "p2", "m1", 0, 100)],
    )
    .await;
    seed::publish(&gproxy).await;
    assert_eq!(order(&gproxy, "named").await, ["p1", "p2"]);

    gproxy
        .store()
        .route_members()
        .update_many(vec![
            gproxy_store::entity::routing::route_member::ActiveModel {
                id: sea_orm::Set("m-a".into()),
                enabled: sea_orm::Set(false),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    seed::publish(&gproxy).await;
    assert_eq!(order(&gproxy, "named").await, ["p2"]);
}
