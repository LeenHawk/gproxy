use super::support::*;
use RealtimeEventKind as K;

#[test]
fn setup_ready_vad_turn_transcripts_usage_and_session_controls() {
    futures_executor::block_on(async {
        let client = Client::new(Script::fixture(include_str!("fixtures/gemini-turn.jsonl")));
        let mut c = config(RealtimeDialect::Gemini);
        c.input_transcription = Some(RealtimeInputTranscription::default());
        c.history = Some(Conversation::new(vec![Message {
            role: Role::User,
            content: vec![Block::text("prior text")],
        }]));
        let mut session = client.connect_realtime(&c).await.unwrap();
        assert!(session.sender.send_text("too early").await.is_err());
        assert_eq!(client.backend.values().len(), 1);
        let setup = &client.backend.values()[0]["setup"];
        assert_eq!(setup["model"], "models/provider/model");
        assert!(setup["outputAudioTranscription"].is_object());
        let mut collector = RealtimeCollector::new();
        assert!(matches!(
            next(&mut session, &mut collector).await.kind,
            K::Ready
        ));
        assert_eq!(
            client.backend.values()[1]["clientContent"]["turns"][0]["parts"][0]["text"],
            "prior text"
        );
        assert_eq!(
            client.backend.values()[1]["clientContent"]["turnComplete"],
            false
        );
        assert_eq!(
            session.sender.update_session(&c).await.unwrap_err().kind,
            ErrorKind::Unsupported
        );
        assert_eq!(
            session.sender.clear().await.unwrap_err().kind,
            ErrorKind::Unsupported
        );
        assert_eq!(
            session.sender.cancel().await.unwrap_err().kind,
            ErrorKind::Unsupported
        );
        let mut events = Vec::new();
        while let Some(event) = session.events.next().await {
            collector.push(&event);
            events.push(event.kind);
        }
        let sequence = events
            .iter()
            .map(|k| {
                serde_json::to_value(k).unwrap()["type"]
                    .as_str()
                    .unwrap_or("native")
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            sequence,
            [
                "speech_started",
                "speech_stopped",
                "input_committed",
                "input_transcript_delta",
                "input_transcript_done",
                "response_started",
                "audio_delta",
                "output_transcript_delta",
                "output_transcript_delta",
                "output_transcript_done",
                "native",
                "response_done",
                "go_away",
                "resumption",
                "closed"
            ]
        );
        assert!(events.iter().any(|e| matches!(
            e,
            K::SpeechStarted {
                audio_ms: Some(0),
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            K::SpeechStopped {
                audio_ms: Some(1100),
                ..
            }
        )));
        assert!(
            events.iter().any(
                |e| matches!(e, K::ResponseDone { usage, .. } if usage.total_tokens == Some(15))
            )
        );
        assert!(
            events.iter().any(
                |e| matches!(e, K::GoAway { time_left } if time_left.as_deref() == Some("30s"))
            )
        );
        assert!(events.iter().any(|e| matches!(e, K::Resumption { handle, resumable: Some(true) } if handle.as_deref() == Some("resume-1"))));
        let (messages, archive) = roundtrip(collector.into_conversation());
        assert_eq!(messages.len(), 2);
        assert!(matches!(&messages[0].content[0].content, Content::Text(t) if t.text == "Hi!"));
        assert!(matches!(&messages[1].content[0].content, Content::Text(t) if t.text == "Hello!"));
        assert!(!archive.contains("AQIDBA=="));
        let h = client.backend.handshake.lock().unwrap();
        let h = h.as_ref().unwrap();
        assert_eq!(h.model, "provider/model");
        assert_eq!(h.operation.dialect, protocol::Dialect::Gemini);
        assert_eq!(
            h.path,
            "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"
        );
    });
}
#[test]
fn manual_audio_boundaries_and_local_interruption_drop_late_audio_and_transcripts() {
    futures_executor::block_on(async {
        let client = Client::new(Script::new(vec![
            json!({"setupComplete":{}}),
            json!({"serverContent":{"modelTurn":{"parts":[{"inlineData":{"mimeType":"audio/pcm;rate=24000","data":"AQIDBA=="}}]},"outputTranscription":{"text":"discard all"}}}),
            json!({"serverContent":{"modelTurn":{"parts":[{"inlineData":{"mimeType":"audio/pcm;rate=24000","data":"BQY="}}]},"outputTranscription":{"text":" including late output","finished":true},"interrupted":true}}),
            json!({"serverContent":{"turnComplete":true}}),
        ]));
        let mut c = config(RealtimeDialect::Gemini);
        c.turn_detection = TurnDetection::Manual;
        let mut session = client.connect_realtime(&c).await.unwrap();
        let mut collector = RealtimeCollector::new();
        next(&mut session, &mut collector).await;
        session.sender.append_audio(&[1, 2]).await.unwrap();
        session.sender.append_audio(&[3, 4]).await.unwrap();
        session.sender.commit().await.unwrap();
        loop {
            if matches!(
                next(&mut session, &mut collector).await.kind,
                K::OutputTranscriptDelta { .. }
            ) {
                break;
            }
        }
        let before = client.backend.values().len();
        session.sender.interrupt(80).await.unwrap();
        let mut audio_after = 0;
        let mut done = false;
        while let Some(event) = session.events.next().await {
            collector.push(&event);
            if matches!(event.kind, K::AudioDelta { .. }) {
                audio_after += 1;
            }
            if matches!(event.kind, K::ResponseDone { finish_reason, .. } if finish_reason.unified == FinishReasonKind::Interrupted)
            {
                done = true;
            }
        }
        assert!(done);
        assert_eq!(audio_after, 0);
        assert_eq!(client.backend.values().len(), before);
        let sent = client.backend.values();
        assert!(sent[1]["realtimeInput"]["activityStart"].is_object());
        assert_eq!(
            sent[2]["realtimeInput"]["audio"]["mimeType"],
            "audio/pcm;rate=16000"
        );
        assert_eq!(sent[3]["realtimeInput"]["audio"]["data"], "AwQ=");
        assert!(sent[4]["realtimeInput"]["activityEnd"].is_object());
        let (messages, archive) = roundtrip(collector.into_conversation());
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].content[0].content,
            Content::Interrupted(_)
        ));
        assert!(messages[0].content[0].provider_metadata.is_empty());
        assert_eq!(
            messages[0].content[0].content,
            Content::Interrupted(Interruption {
                played_ms: Some(80)
            })
        );
        assert!(archive.contains("\"played_ms\":80"));
        assert!(!archive.contains("discard all"));
        assert!(!archive.contains("late output"));
    });
}
#[test]
fn native_interruption_keeps_unknown_playback_duration_and_does_not_affect_next_turn() {
    futures_executor::block_on(async {
        let client = Client::new(Script::new(vec![
            json!({"setupComplete":{}}),
            json!({"serverContent":{"outputTranscription":{"text":"discard this","finished":true}}}),
            json!({"serverContent":{"interrupted":true,"turnComplete":true}}),
            json!({"serverContent":{"outputTranscription":{"text":"keep next","finished":true},"turnComplete":true}}),
        ]));
        let mut events = client
            .connect_realtime(&config(RealtimeDialect::Gemini))
            .await
            .unwrap()
            .events;
        let mut collector = RealtimeCollector::new();
        while let Some(e) = events.next().await {
            collector.push(&e);
        }
        let (messages, archive) = roundtrip(collector.into_conversation());
        assert_eq!(messages.len(), 2);
        assert!(!archive.contains("discard this"));
        assert!(archive.contains("keep next"));
        assert!(archive.contains("\"played_ms\":null"));
    });
}
#[test]
fn tools_cancellation_native_extensions_and_fatal_error() {
    futures_executor::block_on(async {
        let extension = json!({"futureControl":{"v":1}});
        let client = Client::new(Script::new(vec![
            json!({"setupComplete":{}}),
            json!({"toolCall":{"functionCalls":[{"id":"call-1","name":"weather","args":{"city":"Paris"}}]}}),
            json!({"toolCallCancellation":{"ids":["call-1"]}}),
            extension.clone(),
            json!({"usageMetadata":{"totalTokenCount":3}}),
            json!({"error":{"code":403,"status":"PERMISSION_DENIED","message":"denied"}}),
        ]));
        let mut c = config(RealtimeDialect::Gemini);
        c.auto_response = false;
        let mut session = client.connect_realtime(&c).await.unwrap();
        let mut collector = RealtimeCollector::new();
        loop {
            if matches!(
                next(&mut session, &mut collector).await.kind,
                K::ToolCall { .. }
            ) {
                break;
            }
        }
        session
            .sender
            .send_tool_result(&ToolResult {
                id: "call-1".into(),
                name: "weather".into(),
                output: ToolOutput::Json(json!(20)),
                is_error: false,
                provider_executed: false,
            })
            .await
            .unwrap();
        let mut kinds = Vec::new();
        while let Some(e) = session.events.next().await {
            collector.push(&e);
            kinds.push(e.kind);
        }
        assert!(
            kinds
                .iter()
                .any(|e| matches!(e, K::ToolCallCancelled { ids } if ids == &["call-1"]))
        );
        assert!(
            kinds
                .iter()
                .any(|e| matches!(e, K::Warning { warning } if warning.field == "auto_response"))
        );
        assert!(
            kinds
                .iter()
                .any(|e| matches!(e, K::Unknown(v) if v == &extension))
        );
        assert!(
            kinds
                .iter()
                .any(|e| matches!(e, K::Native { event } if event == "usageMetadata"))
        );
        assert!(kinds.iter().any(|e| matches!(e, K::Error { fatal: true, error } if error.kind == ErrorKind::Auth && error.status.is_none())));
        let sent = client.backend.values();
        assert_eq!(
            sent[1]["toolResponse"]["functionResponses"][0]["response"]["result"],
            20
        );
        assert_eq!(
            sent[1]["toolResponse"]["functionResponses"][0]["id"],
            "call-1"
        );
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
