use super::*;
use gproxy_store::entity::{
    limits::{quota, quota_window},
    pricing::{price_rate, price_rule, price_unit::PriceUnit},
    routing::{route, route_member},
    usage::upstream_record,
};

async fn retry_host() -> Host {
    let host = responses_instance("claude", Some(1)).await;
    let handle = host.handle();
    support::credential(&handle, "c2", "p1", None, None, None).await;
    support::provider(&handle, "p2", &["m2"]).await;
    support::credential(&handle, "c3", "p2", None, None, None).await;
    gproxy_store::entity::upstream::provider::Entity::update(
        gproxy_store::entity::upstream::provider::ActiveModel {
            id: Set("p2".into()),
            config: Set(json!({"test_dialects":["openai","openai_responses_websocket"]})),
            ..Default::default()
        },
    )
    .exec(handle.store().connection())
    .await
    .unwrap();
    support::exposed(&handle, "retry", "p1", "m1").await;
    route::Entity::update(route::ActiveModel {
        id: Set("retry".into()),
        max_attempts: Set(3),
        ..Default::default()
    })
    .exec(handle.store().connection())
    .await
    .unwrap();
    handle
        .store()
        .route_members()
        .create_many(vec![route_member::ActiveModel {
            id: Set("retry-m2".into()),
            route_id: Set("retry".into()),
            provider_id: Set("p2".into()),
            upstream_model: Set("m2".into()),
            tier: Set(1),
            enabled: Set(true),
            ..Default::default()
        }])
        .await
        .unwrap();
    host.publish().await;
    host
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_http_retries_credentials_and_providers_then_reconnects_to_the_accepted_target() {
    let host = retry_host().await;
    let handle = host.handle();
    handle
        .store()
        .quotas()
        .create_many(vec![quota::ActiveModel {
            id: Set("m2-budget".into()),
            owner_kind: Set("user".into()),
            owner_id: Set("alice".into()),
            window_key: Set("m2-budget".into()),
            metric: Set("cost".into()),
            unit: Set("USD".into()),
            limit_value: Set("100".parse().unwrap()),
            period: Set("total".into()),
            model_pattern: Set(Some("m2".into())),
            ..Default::default()
        }])
        .await
        .unwrap();
    handle
        .store()
        .price_rules()
        .create_many(vec![price_rule::ActiveModel {
            id: Set("m2-price".into()),
            provider_id: Set(Some("p2".into())),
            model_pattern: Set("m2".into()),
            currency: Set("USD".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    handle
        .store()
        .price_rates()
        .create_many(vec![price_rate::ActiveModel {
            id: Set("m2-input".into()),
            price_rule_id: Set("m2-price".into()),
            metric: Set("input_tokens".into()),
            unit: Set(PriceUnit::Token),
            unit_quantity: Set("1".parse().unwrap()),
            value: Set("1".parse().unwrap()),
            ..Default::default()
        }])
        .await
        .unwrap();
    host.publish().await;
    let (first, rx) = tokio::sync::mpsc::unbounded_channel();
    let (second, rx2) = tokio::sync::mpsc::unbounded_channel();
    host.client.script(vec![
        support::Reply::Http(
            StatusCode::TOO_MANY_REQUESTS,
            json!({"error":{"type":"rate_limit_error","message":"busy"}}),
        ),
        support::Reply::Http(
            StatusCode::UNAUTHORIZED,
            json!({"error":{"type":"authentication_error","message":"expired"}}),
        ),
        support::Reply::Sse(StatusCode::OK, rx),
        support::Reply::Sse(StatusCode::OK, rx2),
    ]);
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
        .await
        .unwrap();
    ws_send(
        &mut client,
        json!({"type":"response.create","model":"retry","input":"start"}),
    )
    .await;
    finish_http_segment(first, response_item("a", "first"), 3, 2);
    let terminal = until_type(&mut client, "response.completed").await;
    assert_eq!(
        host.client.urls(),
        [
            "https://p1.example/v1/messages",
            "https://p1.example/v1/messages",
            "https://p2.example/v1/responses"
        ]
    );
    let records = upstream_record::Entity::find()
        .order_by_asc(upstream_record::Column::AttemptOrdinal)
        .all(host.handle().store().connection())
        .await
        .unwrap();
    assert_eq!(records.len(), 3);
    assert_ne!(records[0].credential_id, records[1].credential_id);
    assert_eq!(records[2].credential_id.as_deref(), Some("c3"));
    assert_eq!(
        records
            .iter()
            .map(|r| r.attempt_ordinal)
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2), Some(3)]
    );
    assert!(
        records
            .iter()
            .all(|r| r.initiator_request_id == records[0].initiator_request_id)
    );
    let usage = usage_rows(&host).await;
    let budget = quota_window::Entity::find()
        .filter(quota_window::Column::QuotaId.eq("m2-budget"))
        .one(handle.store().connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(budget.used, "3".parse().unwrap());
    assert_eq!(
        usage
            .iter()
            .filter(|row| row.input_tokens == Some(3))
            .count(),
        1
    );
    assert_eq!(
        usage
            .iter()
            .find(|row| row.input_tokens == Some(3))
            .unwrap()
            .provider_id
            .as_deref(),
        Some("p2")
    );
    client.close(None).await.unwrap();
    let _ = recv(&mut client).await;

    let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
        .await
        .unwrap();
    ws_send(&mut client, json!({"type":"response.create","model":"retry","previous_response_id":terminal["response"]["id"],"input":"continue"})).await;
    finish_http_segment(second, response_item("b", "second"), 4, 1);
    until_type(&mut client, "response.completed").await;
    assert_eq!(
        host.client.urls().last().unwrap(),
        "https://p2.example/v1/responses"
    );
    let bodies = host.client.bodies();
    assert_eq!(bodies.len(), 4);
    assert!(bodies[3]["previous_response_id"].is_null());
    assert_eq!(bodies[3]["input"].as_array().unwrap().len(), 3);
    assert_eq!(bodies[3]["model"], "m2");
    client.close(None).await.unwrap();
    let _ = recv(&mut client).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_http_final_rejections_keep_status_and_vendor_error_details() {
    for (dialect, status) in [("claude", 429), ("gemini", 503)] {
        let host = responses_instance(dialect, Some(1)).await;
        let detail = if dialect == "gemini" {
            json!({"code":503,"message":"temporarily unavailable","status":"UNAVAILABLE","details":[{"reason":"capacity"}]})
        } else {
            json!({"type":"vendor_error","code":"vendor_code","message":"vendor message","param":"model","extra":{"retry":true}})
        };
        host.client.script(vec![support::Reply::Http(
            StatusCode::from_u16(status).unwrap(),
            json!({"error":detail}),
        )]);
        let bound = host.bind().await;
        let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
            .await
            .unwrap();
        ws_send(
            &mut client,
            json!({"type":"response.create","model":"p1/m1","stream_id":"lane","input":"hello"}),
        )
        .await;
        assert_eq!(ws_json(&mut client).await["type"], "response.created");
        let error = ws_json(&mut client).await;
        assert_eq!(error["type"], "error", "{error}");
        assert_eq!(error["status"], status);
        assert_eq!(error["stream_id"], "lane");
        if dialect == "gemini" {
            assert_eq!(error["error"]["code"], "503");
            assert_eq!(error["error"]["upstream_code"], 503);
            assert_eq!(error["error"]["status"], "UNAVAILABLE");
            assert_eq!(error["error"]["details"], detail["details"]);
        } else {
            assert_eq!(error["error"], detail);
        }
        assert_eq!(host.client.urls().len(), 1);
        client.close(None).await.unwrap();
        let _ = recv(&mut client).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_http_does_not_replay_ambiguous_sends_or_stream_errors() {
    for streaming in [false, true] {
        let host = retry_host().await;
        gproxy_store::entity::upstream::provider::Entity::update(
            gproxy_store::entity::upstream::provider::ActiveModel {
                id: Set("p1".into()),
                config: Set(json!({"test_dialect":"openai"})),
                ..Default::default()
            },
        )
        .exec(host.handle().store().connection())
        .await
        .unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        host.client.script(vec![if streaming {
            support::Reply::Sse(StatusCode::OK, rx)
        } else {
            support::Reply::Failed
        }]);
        let bound = host.bind().await;
        let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
            .await
            .unwrap();
        host.publish().await;
        ws_send(
            &mut client,
            json!({"type":"response.create","model":"retry","input":"hello"}),
        )
        .await;
        assert_eq!(ws_json(&mut client).await["type"], "response.created");
        if streaming {
            tx.send(sse(json!({"type":"response.output_item.done","output_index":0,"item":response_item("a", "paid output")}))).unwrap();
            until_type(&mut client, "response.output_item.done").await;
            tx.send(sse(json!({"type":"error","status":429,"error":{"type":"rate_limit_error","code":"stream_limit","message":"stop"}}))).unwrap();
        }
        let failure = ws_json(&mut client).await;
        assert_eq!(failure["type"], "error", "{failure}");
        if streaming {
            assert_eq!(failure["status"], 429);
            assert_eq!(failure["error"]["code"], "stream_limit");
        }
        assert_eq!(host.client.urls().len(), 1);
        client.close(None).await.unwrap();
        let _ = recv(&mut client).await;
    }
}
