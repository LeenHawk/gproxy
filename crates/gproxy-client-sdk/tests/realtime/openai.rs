use super::support::*;
use RealtimeEventKind as K;

#[test]
fn vad_turn_dispatch_and_transcript_archive() {
    futures_executor::block_on(async {
        let client = Client::new(Script::fixture(include_str!("fixtures/openai-turn.jsonl")));
        let mut session = client
            .connect_realtime(&config(RealtimeDialect::Openai))
            .await
            .unwrap();
        assert_eq!(
            session.sender.append_audio(&[1]).await.unwrap_err().kind,
            ErrorKind::InvalidData
        );
        let mut collector = RealtimeCollector::new();
        let mut kinds = Vec::new();
        while let Some(event) = session.events.next().await {
            assert_eq!(event.metadata.vendor, Vendor::Openai);
            collector.push(&event);
            kinds.push(event.kind);
        }
        assert!(matches!(kinds[0], K::Unknown(_)));
        assert!(matches!(kinds[1], K::Ready));
        let sequence = kinds
            .iter()
            .map(|k| {
                serde_json::to_value(k).unwrap()["type"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            sequence,
            [
                "session.created",
                "ready",
                "speech_started",
                "speech_stopped",
                "input_committed",
                "input_transcript_delta",
                "input_transcript_done",
                "response_started",
                "audio_delta",
                "output_transcript_delta",
                "output_transcript_done",
                "response_done",
                "closed"
            ]
        );
        assert!(kinds.iter().any(|k| matches!(
            k,
            K::SpeechStarted {
                audio_ms: Some(0),
                ..
            }
        )));
        assert!(kinds.iter().any(|k| matches!(
            k,
            K::SpeechStopped {
                audio_ms: Some(1100),
                ..
            }
        )));
        assert!(kinds.iter().any(|k| matches!(k, K::AudioDelta { bytes, format, .. } if bytes == &[1,2,3,4] && format == &AudioFormat::pcm(24000,1))));
        assert!(kinds.iter().any(|k| matches!(k, K::ResponseDone { usage, finish_reason, .. } if usage.total_tokens == Some(15) && usage.tokens.cache_read_tokens == Some(2) && finish_reason.unified == FinishReasonKind::Stop)));
        let (messages, archive) = roundtrip(collector.into_conversation());
        assert_eq!(messages.len(), 2);
        assert!(matches!(&messages[0].content[0].content, Content::Text(t) if t.text == "Hi!"));
        assert!(matches!(&messages[1].content[0].content, Content::Text(t) if t.text == "Hello!"));
        assert!(archive.contains("realtime_speech_transcript"));
        assert!(!archive.contains("AQIDBA=="));
        let handshake = client.backend.handshake.lock().unwrap();
        let h = handshake.as_ref().unwrap();
        assert_eq!(h.model, "provider/model");
        assert_eq!(h.path, "/v1/realtime");
        assert_eq!(h.query.as_deref(), Some("model=provider%2Fmodel"));
        assert_eq!(h.operation.operation, protocol::Operation::ConnectRealtime);
        assert_eq!(h.operation.dialect, protocol::Dialect::OpenAi);
        assert_eq!(client.backend.values()[0]["type"], "session.update");
    });
}
#[test]
fn interrupt_sends_cancel_and_truncate_clears_playback_and_forgets_all_speech() {
    futures_executor::block_on(async {
        let frames = vec![
            ready(),
            json!({"type":"response.created","event_id":"r","response":{"id":"response-1","status":"in_progress"}}),
            output("response.output_audio.delta", "delta", json!("AQIDBA==")),
            output(
                "response.output_audio_transcript.delta",
                "delta",
                json!("heard and unheard"),
            ),
            output(
                "response.output_audio_transcript.done",
                "transcript",
                json!("heard and unheard, including late words"),
            ),
            json!({"type":"conversation.item.truncated","event_id":"tr","item_id":"answer","content_index":0,"audio_end_ms":120}),
            json!({"type":"response.done","event_id":"done","response":{"id":"response-1","status":"cancelled"}}),
        ];
        let client = Client::new(Script::new(frames));
        let mut c = config(RealtimeDialect::Openai);
        c.turn_detection = TurnDetection::Manual;
        let mut session = client.connect_realtime(&c).await.unwrap();
        let mut collector = RealtimeCollector::new();
        assert!(matches!(
            next(&mut session, &mut collector).await.kind,
            K::Ready
        ));
        let sender = session.sender.clone();
        sender.append_audio(&[1, 2]).await.unwrap();
        sender.commit().await.unwrap();
        sender.clear().await.unwrap();
        sender.send_text("typed input").await.unwrap();
        sender.create_response().await.unwrap();
        loop {
            if matches!(
                next(&mut session, &mut collector).await.kind,
                K::OutputTranscriptDelta { .. }
            ) {
                break;
            }
        }
        sender.interrupt(120).await.unwrap();
        assert!(matches!(
            next(&mut session, &mut collector).await.kind,
            K::ClearLocalPlayback
        ));
        assert!(matches!(
            next(&mut session, &mut collector).await.kind,
            K::Interrupted {
                played_ms: Some(120),
                ..
            }
        ));
        while let Some(event) = session.events.next().await {
            collector.push(&event);
        }
        let (_, archive) = roundtrip(collector.into_conversation());
        assert!(!archive.contains("heard"));
        assert!(archive.contains("typed input"));
        assert!(archive.contains("\"played_ms\":120"));
        assert!(archive.contains("\"type\":\"interrupted\""));
        let sent = client.backend.values();
        let tags = sent
            .iter()
            .map(|v| v["type"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            &tags[tags.len() - 2..],
            &["response.cancel", "conversation.item.truncate"]
        );
        assert!(!tags.contains(&"output_audio_buffer.clear"));
        assert_eq!(sent.last().unwrap()["item_id"], "answer");
        assert_eq!(sent.last().unwrap()["audio_end_ms"], 120);
        assert_eq!(sent[1]["type"], "input_audio_buffer.append");
        assert_eq!(sent[2]["type"], "input_audio_buffer.commit");
        assert_eq!(sent[3]["type"], "input_audio_buffer.clear");
    });
}
#[test]
fn tool_round_trip_and_optional_automatic_response_creation() {
    futures_executor::block_on(async {
        let frames = vec![
            ready(),
            json!({"type":"response.created","event_id":"r","response":{"id":"r1"}}),
            json!({"type":"response.function_call_arguments.done","event_id":"t","response_id":"r1","item_id":"tool-item","output_index":0,"call_id":"call-1","name":"weather","arguments":"{\"city\":\"Paris\"}"}),
        ];
        let client = Client::new(Script::new(frames));
        let mut session = client
            .connect_realtime(&config(RealtimeDialect::Openai))
            .await
            .unwrap();
        let mut collector = RealtimeCollector::new();
        loop {
            if matches!(
                next(&mut session, &mut collector).await.kind,
                K::ToolCall { .. }
            ) {
                break;
            }
        }
        let result = ToolResult {
            id: "call-1".into(),
            name: "weather".into(),
            output: ToolOutput::Json(json!({"temperature":20})),
            is_error: false,
            provider_executed: false,
        };
        session.sender.send_tool_result(&result).await.unwrap();
        assert!(matches!(
            next(&mut session, &mut collector).await.kind,
            K::ToolResultSent { .. }
        ));
        session
            .sender
            .send_tool_result_with_response(&result, false)
            .await
            .unwrap();
        let sent = client.backend.values();
        assert_eq!(sent[1]["item"]["type"], "function_call_output");
        assert_eq!(sent[1]["item"]["call_id"], "call-1");
        assert_eq!(sent[2]["type"], "response.create");
        assert_eq!(sent.len(), 4);
        let (messages, _) = roundtrip(collector.into_conversation());
        assert!(matches!(
            messages[0].content[0].content,
            Content::ToolCall(_)
        ));
        assert!(matches!(
            messages[1].content[0].content,
            Content::ToolResult(_)
        ));
    });
}
#[test]
fn transcription_failure_recoverable_error_rate_limit_unknown_and_close_code() {
    futures_executor::block_on(async {
        let unknown = json!({"type":"future.event","payload":{"x":3}});
        let script = Script::new(vec![
            ready(),
            json!({"type":"conversation.item.input_audio_transcription.delta","event_id":"x","item_id":"u","delta":"discard me"}),
            json!({"type":"conversation.item.input_audio_transcription.failed","event_id":"x2","item_id":"u","content_index":0,"error":{"message":"cannot decode","code":"invalid_audio"}}),
            json!({"type":"error","event_id":"err","error":{"type":"invalid_request_error","message":"bad event"}}),
            unknown.clone(),
            json!({"type":"rate_limits.updated","event_id":"limits","rate_limits":[{"name":"tokens","remaining":1,"limit":10,"reset_seconds":2}]}),
        ]);
        script.frames.lock().unwrap().as_mut().unwrap().pop();
        script
            .frames
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .push(protocol::connection::WsFrame::Close(Some(
                protocol::connection::WsClose {
                    code: 1008,
                    reason: "policy".into(),
                },
            )));
        let mut session = Client::new(script)
            .connect_realtime(&config(RealtimeDialect::Openai))
            .await
            .unwrap();
        let mut collector = RealtimeCollector::new();
        let mut kinds = Vec::new();
        while let Some(e) = session.events.next().await {
            collector.push(&e);
            kinds.push(e.kind);
        }
        assert!(kinds.iter().any(
            |k| matches!(k, K::InputTranscriptionFailed { error, .. } if error.status.is_none())
        ));
        assert!(kinds.iter().any(
            |k| matches!(k, K::Error { fatal: false, error } if error.message == "bad event")
        ));
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, K::Unknown(v) if *v == unknown))
        );
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, K::RateLimits { limits } if limits[0]["remaining"] == 1))
        );
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, K::Error { fatal: true, .. }))
        );
        assert!(
            matches!(kinds.last(), Some(K::Closed { code: Some(1008), reason }) if reason == "policy")
        );
        let (_, archive) = roundtrip(collector.into_conversation());
        assert!(!archive.contains("discard me"));
        let persisted: RealtimeEventKind = serde_json::from_value(unknown.clone()).unwrap();
        assert_eq!(serde_json::to_value(persisted).unwrap(), unknown);
    });
}
