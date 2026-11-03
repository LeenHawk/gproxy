use super::*;
use gproxy_store::entity::identity::permission;

const GEMINI: &str =
    "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";

async fn restricted() -> Host {
    let host = instance(None).await;
    let handle = host.handle();
    support::exposed(&handle, "ns/voice", "p1", "m1").await;
    permission::Entity::update(permission::ActiveModel {
        id: Set("perm".into()),
        user_id: Set(None),
        api_key_id: Set(Some("k-alice".into())),
        model_pattern: Set("ns/voice".into()),
        ..Default::default()
    })
    .exec(handle.store().connection())
    .await
    .unwrap();
    host.publish().await;
    host
}

#[tokio::test(flavor = "multi_thread")]
async fn openai_query_is_admitted_and_mapped_before_upstream_upgrade() {
    let host = restricted().await;
    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(
        &bound,
        "/ns/v1/realtime?model=voice&trace=a%2Fb",
        Some("k-alice"),
    )
    .await
    .unwrap();
    assert_eq!(
        host.client.urls(),
        ["https://p1.example/v1/realtime?trace=a%2Fb&model=m1"]
    );
    ws_send(
        &mut client,
        json!({"type":"response.create","model":"client-override"}),
    )
    .await;
    let Some(WsFrame::Text(frame)) = upstream.next().await else {
        panic!("create")
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&frame).unwrap()["model"],
        "m1"
    );
    client.close(None).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn openai_missing_malformed_and_disallowed_models_never_open_upstream() {
    let host = restricted().await;
    let bound = host.bind().await;
    for (query, status) in [
        ("", StatusCode::BAD_REQUEST),
        ("?model=", StatusCode::BAD_REQUEST),
        ("?model=voice&model=other", StatusCode::BAD_REQUEST),
        ("?model=other", StatusCode::FORBIDDEN),
    ] {
        let error = dial(&bound, &format!("/ns/v1/realtime{query}"), Some("k-alice"))
            .await
            .unwrap_err();
        assert!(
            matches!(error, tokio_tungstenite::tungstenite::Error::Http(ref response) if response.status() == status),
            "{error:?}"
        );
    }
    assert!(host.client.urls().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn gemini_waits_for_setup_then_maps_alias_and_preserves_text_or_binary_frames() {
    for (path, binary) in [(GEMINI, false), (GEMINI, true)] {
        let host = restricted().await;
        let upstream = host.client.accept_socket();
        let bound = host.bind().await;
        let mut client = dial(&bound, &format!("/ns{path}"), Some("k-alice"))
            .await
            .unwrap();
        assert!(host.client.urls().is_empty(), "no upstream before setup");
        let setup = json!({"setup":{"model":"models/voice","generationConfig":{"responseModalities":["AUDIO"]},"future":{"keep":3}}});
        if binary {
            client
                .send(Message::Binary(serde_json::to_vec(&setup).unwrap().into()))
                .await
                .unwrap();
        } else {
            ws_send(&mut client, setup).await;
        }
        let received = upstream.next().await.unwrap();
        let bytes = match &received {
            WsFrame::Text(text) => text.as_bytes(),
            WsFrame::Binary(bytes) => bytes.as_ref(),
            _ => panic!("setup"),
        };
        assert_eq!(matches!(received, WsFrame::Binary(_)), binary);
        let value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(value["setup"]["model"], "models/m1");
        assert_eq!(value["setup"]["future"]["keep"], 3);
        assert_eq!(
            value["setup"]["generationConfig"]["responseModalities"],
            json!(["AUDIO"])
        );
        assert_eq!(host.client.urls(), [format!("https://p1.example{path}")]);
        upstream
            .send
            .send(Ok(WsFrame::Text(json!({"setupComplete":{}}).to_string())))
            .unwrap();
        assert!(ws_json(&mut client).await["setupComplete"].is_object());
        ws_send(
            &mut client,
            json!({"realtimeInput":{"audioStreamEnd":true}}),
        )
        .await;
        assert_eq!(
            upstream.next().await,
            Some(WsFrame::Text(
                json!({"realtimeInput":{"audioStreamEnd":true}}).to_string()
            ))
        );
        client.close(None).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn gemini_rejects_invalid_or_unpermitted_setup_without_opening_upstream() {
    let host = restricted().await;
    let bound = host.bind().await;
    for (frame, status) in [
        ("not-json", 400),
        ("{}", 400),
        (r#"{"setup":{}}"#, 400),
        (r#"{"setup":{"model":3}}"#, 400),
        (r#"{"setup":{"model":"models/"}}"#, 400),
        (r#"{"setup":{"model":"models/other"}}"#, 403),
    ] {
        let mut client = dial(&bound, GEMINI, Some("k-alice")).await.unwrap();
        client.send(Message::Text(frame.into())).await.unwrap();
        assert_eq!(ws_json(&mut client).await["error"]["code"], status);
        assert!(
            matches!(recv(&mut client).await, Some(Message::Close(Some(close))) if u16::from(close.code) == 1008)
        );
    }
    assert!(host.client.urls().is_empty());
    let mut client = dial(&bound, GEMINI, Some("k-alice")).await.unwrap();
    client.close(None).await.unwrap();
    assert!(
        host.client.urls().is_empty(),
        "closing before setup creates no upstream session"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn gemini_upstream_refusal_keeps_status_and_body_in_the_post_upgrade_error() {
    let host = instance(None).await;
    host.client.push_socket(WsReply::Rejected(
        StatusCode::FORBIDDEN,
        json!({"error":{"message":"upstream denied"}}),
    ));
    let bound = host.bind().await;
    let mut client = dial(&bound, GEMINI, Some("k-alice")).await.unwrap();
    ws_send(&mut client, json!({"setup":{"model":"models/p1/m1"}})).await;
    let error = ws_json(&mut client).await;
    assert_eq!(error["error"]["code"], 403);
    let body: serde_json::Value =
        serde_json::from_str(error["error"]["message"].as_str().unwrap()).unwrap();
    assert_eq!(body["error"]["message"], "upstream denied");
    assert!(
        matches!(recv(&mut client).await, Some(Message::Close(Some(close))) if u16::from(close.code) == 1008)
    );
}
