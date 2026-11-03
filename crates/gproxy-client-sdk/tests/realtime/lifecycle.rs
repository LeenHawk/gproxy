use super::support::*;
use RealtimeEventKind as K;

#[test]
fn both_realtime_dialects_silently_skip_interruption_history() {
    futures_executor::block_on(async {
        let mut marker = Block::new(Content::Interrupted(Interruption {
            played_ms: Some(80),
        }));
        marker.cache = Some(CacheBreakpoint { ttl: None });
        marker.provider_metadata.insert(
            Vendor::Openai,
            serde_json::from_value(json!({"native":false})).unwrap(),
        );
        marker.provider_metadata.insert(
            Vendor::Gemini,
            serde_json::from_value(json!({"native":false})).unwrap(),
        );
        for dialect in [RealtimeDialect::Openai, RealtimeDialect::Gemini] {
            let mut c = config(dialect.clone());
            let mut messages = vec![
                Message {
                    role: Role::System,
                    content: vec![Block::text("system")],
                },
                Message {
                    role: Role::User,
                    content: vec![Block::text("history")],
                },
            ];
            let mut expected = Vec::new();
            for include_marker in [false, true] {
                if include_marker {
                    for message in &mut messages {
                        message.content.insert(0, marker.clone());
                    }
                    for role in [Role::Assistant, Role::Tool, Role::Unknown("archive".into())] {
                        messages.push(Message {
                            role,
                            content: vec![marker.clone()],
                        });
                    }
                }
                c.history = Some(Conversation::new(messages.clone()));
                let client =
                    Client::new(Script::new(vec![if dialect == RealtimeDialect::Openai {
                        ready()
                    } else {
                        json!({"setupComplete":{}})
                    }]));
                let mut session = client.connect_realtime(&c).await.unwrap();
                assert!(matches!(
                    session.events.next().await.unwrap().kind,
                    K::Ready
                ));
                if include_marker {
                    assert_eq!(client.backend.values(), expected);
                } else {
                    expected = client.backend.values();
                }
            }
        }
    });
}

#[test]
fn interrupting_completed_audio_does_not_cancel_a_newer_text_response() {
    futures_executor::block_on(async {
        let client = Client::new(Script::new(vec![
            ready(),
            json!({"type":"response.created","event_id":"r1","response":{"id":"response-1"}}),
            output("response.output_audio.delta", "delta", json!("AQIDBA==")),
            json!({"type":"response.done","event_id":"done","response":{"id":"response-1","status":"completed"}}),
            json!({"type":"response.created","event_id":"r2","response":{"id":"response-2"}}),
        ]));
        let mut session = client
            .connect_realtime(&config(RealtimeDialect::Openai))
            .await
            .unwrap();
        loop {
            if matches!(session.events.next().await.unwrap().kind, K::ResponseStarted { response_id: Some(id) } if id == "response-2")
            {
                break;
            }
        }
        session.sender.interrupt(0).await.unwrap();
        let sent = client.backend.values();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1]["type"], "conversation.item.truncate");
        assert_eq!(sent[1]["item_id"], "answer");
    });
}

#[test]
fn gemini_vad_boundaries_finish_input_transcripts_without_a_finished_flag() {
    futures_executor::block_on(async {
        let frames = vec![
            json!({"setupComplete":{}}),
            json!({"voiceActivity":{"type":"ACTIVITY_START"}}),
            json!({"serverContent":{"inputTranscription":{"text":"first turn"}}}),
            json!({"voiceActivity":{"type":"ACTIVITY_END"}}),
            json!({"voiceActivity":{"type":"ACTIVITY_START"}}),
            json!({"serverContent":{"inputTranscription":{"text":"second turn","finished":true}}}),
        ];
        let mut stream = Client::new(Script::new(frames))
            .connect_realtime(&config(RealtimeDialect::Gemini))
            .await
            .unwrap()
            .events;
        let mut collector = RealtimeCollector::new();
        while let Some(event) = stream.next().await {
            collector.push(&event);
        }
        let (messages, _) = roundtrip(collector.into_conversation());
        assert_eq!(messages.len(), 2);
        assert!(
            matches!(&messages[0].content[0].content, Content::Text(t) if t.text == "first turn")
        );
        assert!(
            matches!(&messages[1].content[0].content, Content::Text(t) if t.text == "second turn")
        );
    });
}

#[test]
fn server_cancellation_discards_speech_without_poisoning_the_next_text_turn() {
    futures_executor::block_on(async {
        let client = Client::new(Script::new(vec![
            ready(),
            json!({"type":"response.created","event_id":"r","response":{"id":"response-1"}}),
            output(
                "response.output_audio_transcript.done",
                "transcript",
                json!("never archive cancelled speech"),
            ),
            json!({"type":"response.done","event_id":"done","response":{"id":"response-1","status":"cancelled"}}),
            json!({"type":"response.created","event_id":"r2","response":{"id":"response-2"}}),
            output(
                "response.output_text.done",
                "text",
                json!("keep plain text"),
            ),
            json!({"type":"response.done","event_id":"done2","response":{"id":"response-2","status":"completed"}}),
        ]));
        let mut events = client
            .connect_realtime(&config(RealtimeDialect::Openai))
            .await
            .unwrap()
            .events;
        let mut collector = RealtimeCollector::new();
        let mut finishes = Vec::new();
        while let Some(e) = events.next().await {
            collector.push(&e);
            if let K::ResponseDone { finish_reason, .. } = e.kind {
                finishes.push(finish_reason.unified);
            }
        }
        assert_eq!(
            finishes,
            [FinishReasonKind::Interrupted, FinishReasonKind::Stop]
        );
        let (messages, archive) = roundtrip(collector.into_conversation());
        assert!(!archive.contains("never archive"));
        assert!(archive.contains("keep plain text"));
        assert!(matches!(
            messages[0].content[0].content,
            Content::Interrupted(_)
        ));
        assert_eq!(
            messages[1].content[0].provider_metadata[&Vendor::Openai]["realtime_finish_reason"]["unified"],
            "stop"
        );
    });
}
#[test]
fn session_acknowledgement_controls_output_format_and_native_sends_are_typed() {
    futures_executor::block_on(async {
        let mut update = ready();
        update["session"]["audio"] = json!({"output":{"format":{"type":"audio/pcmu"}}});
        let client = Client::new(Script::new(vec![
            ready(),
            update,
            output("response.output_audio.delta", "delta", json!("AQID")),
        ]));
        let c = config(RealtimeDialect::Openai);
        let mut session = client.connect_realtime(&c).await.unwrap();
        session.events.next().await.unwrap();
        let mut changed = c.clone();
        changed.output_audio_format = AudioFormat::Encoded {
            media_type: "audio/pcmu".into(),
        };
        session.sender.update_session(&changed).await.unwrap();
        assert!(matches!(
            session.events.next().await.unwrap().kind,
            K::Updated
        ));
        assert!(
            matches!(session.events.next().await.unwrap().kind, K::AudioDelta { format: AudioFormat::Encoded { media_type }, .. } if media_type == "audio/pcmu")
        );
        use protocol::openai::realtime as o;
        session
            .sender
            .send_native(NativeRealtimeMessage::Openai(
                &o::RealtimeClientEvent::InputAudioBufferClearEvent(
                    o::InputAudioBufferClearEvent::builder(
                        o::InputAudioBufferClearEventType::InputAudioBufferClear,
                    )
                    .build(),
                ),
            ))
            .await
            .unwrap();
        let future = json!({"type":"future.control","value":3});
        session
            .sender
            .send_native(NativeRealtimeMessage::Raw(&future))
            .await
            .unwrap();
        let sent = client.backend.values();
        assert_eq!(
            sent[1]["session"]["audio"]["output"]["format"]["type"],
            "audio/pcmu"
        );
        assert_eq!(sent[2]["type"], "input_audio_buffer.clear");
        assert_eq!(sent[3], future);
        changed.model.id = "other".into();
        assert!(session.sender.update_session(&changed).await.is_err());
    });
}
#[test]
fn gemini_system_history_maps_into_setup_and_raw_setup_cannot_be_repeated() {
    futures_executor::block_on(async {
        let client = Client::new(Script::new(vec![json!({"setupComplete":{}})]));
        let mut c = config(RealtimeDialect::Gemini);
        c.instructions = Some("portable".into());
        c.history = Some(Conversation::new(vec![Message {
            role: Role::System,
            content: vec![Block::text("history system")],
        }]));
        let mut session = client.connect_realtime(&c).await.unwrap();
        let setup = client.backend.values();
        assert_eq!(
            setup[0]["setup"]["systemInstruction"]["parts"][0]["text"],
            "portable"
        );
        assert_eq!(
            setup[0]["setup"]["systemInstruction"]["parts"][1]["text"],
            "history system"
        );
        session.events.next().await.unwrap();
        assert_eq!(client.backend.values().len(), 1);
        assert_eq!(
            session
                .sender
                .send_native(NativeRealtimeMessage::Raw(
                    &json!({"setup":{"model":"models/other"}})
                ))
                .await
                .unwrap_err()
                .kind,
            ErrorKind::Unsupported
        );
    });
}
#[test]
fn idle_eof_drops_an_unfinished_transcript_and_archives_only_the_marker() {
    futures_executor::block_on(async {
        let script = Script::new(vec![
            ready(),
            output(
                "response.output_audio_transcript.delta",
                "delta",
                json!("unfinished"),
            ),
        ]);
        script.frames.lock().unwrap().as_mut().unwrap().pop();
        let mut stream = Client::new(script)
            .connect_realtime(&config(RealtimeDialect::Openai))
            .await
            .unwrap()
            .events;
        let mut collector = RealtimeCollector::new();
        while let Some(event) = stream.next().await {
            collector.push(&event);
        }
        let (_, archive) = roundtrip(collector.into_conversation());
        assert!(!archive.contains("unfinished"));
        assert!(archive.contains("\"type\":\"interrupted\""));
    });
}
