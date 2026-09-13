#![recursion_limit = "512"]
//! Fixtures independently derived from the official SDK snapshot listed in Realtime types.
//! Each case records the upstream required/nullable field sets, not Rust field reflection.
use gproxy_protocol::openai::realtime::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
fn round_trip<T: Serialize + DeserializeOwned + std::fmt::Debug>(value: Value, known: bool) {
    let parsed: T =
        serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{value}: {e}"));
    if known {
        assert!(
            format!("{parsed:?}")
                .split("rest: ")
                .skip(1)
                .all(|r| r.starts_with("{}")),
            "known fields entered rest: {parsed:?}"
        );
    }
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}
fn object<T: Serialize + DeserializeOwned + std::fmt::Debug>(
    wire: &str,
    required: &[&str],
    nullable: &[&str],
) {
    let value: Value = serde_json::from_str(wire).unwrap();
    round_trip::<T>(value.clone(), true);
    let mut minimal = value.clone();
    minimal
        .as_object_mut()
        .unwrap()
        .retain(|key, _| required.contains(&key.as_str()));
    round_trip::<T>(minimal.clone(), true);
    minimal["future_extension"] = json!({"opaque":null});
    round_trip::<T>(minimal, false);
    for field in required {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(*field);
        assert!(
            serde_json::from_value::<T>(missing).is_err(),
            "required {field} in {wire}"
        );
    }
    for field in nullable {
        let mut null = value.clone();
        null[*field] = Value::Null;
        round_trip::<T>(null, true);
    }
}
fn literal<T: Serialize + DeserializeOwned + std::fmt::Debug>(values: &[Value]) {
    for value in values {
        round_trip::<T>(value.clone(), true);
    }
    assert!(serde_json::from_value::<T>(json!("UNDOCUMENTED_LITERAL")).is_err());
}

#[test]
fn every_upstream_object_field_required_and_nullable_contract() {
    object::<RealtimeSessionCreateRequest>(
        r#"{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"}"#,
        &["type"],
        &[
            "audio",
            "include",
            "instructions",
            "max_output_tokens",
            "model",
            "output_modalities",
            "parallel_tool_calls",
            "prompt",
            "reasoning",
            "tool_choice",
            "tools",
            "tracing",
            "truncation",
        ],
    );
    object::<RealtimeSessionCreateResponse>(
        r#"{"id":"sample","object":"realtime.session","type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"expires_at":1,"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"}"#,
        &["id", "object", "type"],
        &[
            "audio",
            "expires_at",
            "include",
            "instructions",
            "max_output_tokens",
            "model",
            "output_modalities",
            "prompt",
            "reasoning",
            "tool_choice",
            "tools",
            "tracing",
            "truncation",
        ],
    );
    object::<RealtimeTranscriptionSessionCreateResponse>(
        r#"{"id":"sample","object":"sample","type":"transcription","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5,"type":"sample"}}},"expires_at":1,"include":["item.input_audio_transcription.logprobs"]}"#,
        &["id", "object", "type"],
        &["audio", "expires_at", "include"],
    );
    object::<ConversationItemCreateEvent>(
        r#"{"item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.create","event_id":"sample","previous_item_id":"sample"}"#,
        &["item", "type"],
        &["event_id", "previous_item_id"],
    );
    object::<ConversationItemDeleteEvent>(
        r#"{"item_id":"sample","type":"conversation.item.delete","event_id":"sample"}"#,
        &["item_id", "type"],
        &["event_id"],
    );
    object::<ConversationItemRetrieveEvent>(
        r#"{"item_id":"sample","type":"conversation.item.retrieve","event_id":"sample"}"#,
        &["item_id", "type"],
        &["event_id"],
    );
    object::<ConversationItemTruncateEvent>(
        r#"{"audio_end_ms":1,"content_index":1,"item_id":"sample","type":"conversation.item.truncate","event_id":"sample"}"#,
        &["audio_end_ms", "content_index", "item_id", "type"],
        &["event_id"],
    );
    object::<InputAudioBufferAppendEvent>(
        r#"{"audio":"sample","type":"input_audio_buffer.append","event_id":"sample"}"#,
        &["audio", "type"],
        &["event_id"],
    );
    object::<InputAudioBufferClearEvent>(
        r#"{"type":"input_audio_buffer.clear","event_id":"sample"}"#,
        &["type"],
        &["event_id"],
    );
    object::<OutputAudioBufferClearEvent>(
        r#"{"type":"output_audio_buffer.clear","event_id":"sample"}"#,
        &["type"],
        &["event_id"],
    );
    object::<InputAudioBufferCommitEvent>(
        r#"{"type":"input_audio_buffer.commit","event_id":"sample"}"#,
        &["type"],
        &["event_id"],
    );
    object::<ResponseCancelEvent>(
        r#"{"type":"response.cancel","event_id":"sample","response_id":"sample"}"#,
        &["type"],
        &["event_id", "response_id"],
    );
    object::<ResponseCreateEvent>(
        r#"{"type":"response.create","event_id":"sample","response":{"audio":{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}},"conversation":"sample","input":[{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}],"instructions":"sample","max_output_tokens":1,"metadata":{"sample":"sample"},"output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}]}}"#,
        &["type"],
        &["event_id", "response"],
    );
    object::<SessionUpdateEvent>(
        r#"{"session":{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"},"type":"session.update","event_id":"sample"}"#,
        &["session", "type"],
        &["event_id"],
    );
    object::<ConversationCreatedEvent>(
        r#"{"conversation":{"id":"sample","object":"realtime.conversation"},"event_id":"sample","type":"conversation.created"}"#,
        &["conversation", "event_id", "type"],
        &[],
    );
    object::<ConversationItemCreatedEvent>(
        r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.created","previous_item_id":"sample"}"#,
        &["event_id", "item", "type"],
        &["previous_item_id"],
    );
    object::<ConversationItemDeletedEvent>(
        r#"{"event_id":"sample","item_id":"sample","type":"conversation.item.deleted"}"#,
        &["event_id", "item_id", "type"],
        &[],
    );
    object::<ConversationItemInputAudioTranscriptionCompletedEvent>(
        r#"{"content_index":1,"event_id":"sample","item_id":"sample","transcript":"sample","type":"conversation.item.input_audio_transcription.completed","usage":{"input_tokens":1,"output_tokens":1,"total_tokens":1,"type":"tokens","input_token_details":{"audio_tokens":1,"text_tokens":1}},"languages":[{"code":"sample"}],"logprobs":[{"token":"sample","bytes":[1],"logprob":0.5}]}"#,
        &[
            "content_index",
            "event_id",
            "item_id",
            "transcript",
            "type",
            "usage",
        ],
        &["languages", "logprobs"],
    );
    object::<ConversationItemInputAudioTranscriptionDeltaEvent>(
        r#"{"event_id":"sample","item_id":"sample","type":"conversation.item.input_audio_transcription.delta","content_index":1,"delta":"sample","logprobs":[{"token":"sample","bytes":[1],"logprob":0.5}]}"#,
        &["event_id", "item_id", "type"],
        &["content_index", "delta", "logprobs"],
    );
    object::<ConversationItemInputAudioTranscriptionFailedEvent>(
        r#"{"content_index":1,"error":{"code":"sample","message":"sample","param":"sample","type":"sample"},"event_id":"sample","item_id":"sample","type":"conversation.item.input_audio_transcription.failed"}"#,
        &["content_index", "error", "event_id", "item_id", "type"],
        &[],
    );
    object::<ConversationItemRetrieved>(
        r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.retrieved"}"#,
        &["event_id", "item", "type"],
        &[],
    );
    object::<ConversationItemTruncatedEvent>(
        r#"{"audio_end_ms":1,"content_index":1,"event_id":"sample","item_id":"sample","type":"conversation.item.truncated"}"#,
        &[
            "audio_end_ms",
            "content_index",
            "event_id",
            "item_id",
            "type",
        ],
        &[],
    );
    object::<RealtimeErrorEvent>(
        r#"{"error":{"message":"sample","type":"sample","code":"sample","event_id":"sample","param":"sample"},"event_id":"sample","type":"error"}"#,
        &["error", "event_id", "type"],
        &[],
    );
    object::<InputAudioBufferClearedEvent>(
        r#"{"event_id":"sample","type":"input_audio_buffer.cleared"}"#,
        &["event_id", "type"],
        &[],
    );
    object::<InputAudioBufferCommittedEvent>(
        r#"{"event_id":"sample","item_id":"sample","type":"input_audio_buffer.committed","previous_item_id":"sample"}"#,
        &["event_id", "item_id", "type"],
        &["previous_item_id"],
    );
    object::<InputAudioBufferDtmfEventReceivedEvent>(
        r#"{"event":"sample","received_at":1,"type":"input_audio_buffer.dtmf_event_received"}"#,
        &["event", "received_at", "type"],
        &[],
    );
    object::<InputAudioBufferSpeechStartedEvent>(
        r#"{"audio_start_ms":1,"event_id":"sample","item_id":"sample","type":"input_audio_buffer.speech_started"}"#,
        &["audio_start_ms", "event_id", "item_id", "type"],
        &[],
    );
    object::<InputAudioBufferSpeechStoppedEvent>(
        r#"{"audio_end_ms":1,"event_id":"sample","item_id":"sample","type":"input_audio_buffer.speech_stopped"}"#,
        &["audio_end_ms", "event_id", "item_id", "type"],
        &[],
    );
    object::<RateLimitsUpdatedEvent>(
        r#"{"event_id":"sample","rate_limits":[{"limit":1,"name":"requests","remaining":1,"reset_seconds":0.5}],"type":"rate_limits.updated"}"#,
        &["event_id", "rate_limits", "type"],
        &[],
    );
    object::<ResponseAudioDeltaEvent>(
        r#"{"content_index":1,"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_audio.delta"}"#,
        &[
            "content_index",
            "delta",
            "event_id",
            "item_id",
            "output_index",
            "response_id",
            "type",
        ],
        &[],
    );
    object::<ResponseAudioDoneEvent>(
        r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_audio.done"}"#,
        &[
            "content_index",
            "event_id",
            "item_id",
            "output_index",
            "response_id",
            "type",
        ],
        &[],
    );
    object::<ResponseAudioTranscriptDeltaEvent>(
        r#"{"content_index":1,"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_audio_transcript.delta"}"#,
        &[
            "content_index",
            "delta",
            "event_id",
            "item_id",
            "output_index",
            "response_id",
            "type",
        ],
        &[],
    );
    object::<ResponseAudioTranscriptDoneEvent>(
        r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","transcript":"sample","type":"response.output_audio_transcript.done"}"#,
        &[
            "content_index",
            "event_id",
            "item_id",
            "output_index",
            "response_id",
            "transcript",
            "type",
        ],
        &[],
    );
    object::<ResponseContentPartAddedEvent>(
        r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"part":{"audio":"sample","text":"sample","transcript":"sample","type":"text"},"response_id":"sample","type":"response.content_part.added"}"#,
        &[
            "content_index",
            "event_id",
            "item_id",
            "output_index",
            "part",
            "response_id",
            "type",
        ],
        &[],
    );
    object::<ResponseContentPartDoneEvent>(
        r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"part":{"audio":"sample","text":"sample","transcript":"sample","type":"text"},"response_id":"sample","type":"response.content_part.done"}"#,
        &[
            "content_index",
            "event_id",
            "item_id",
            "output_index",
            "part",
            "response_id",
            "type",
        ],
        &[],
    );
    object::<ResponseCreatedEvent>(
        r#"{"event_id":"sample","response":{"id":"sample","audio":{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}},"conversation_id":"sample","max_output_tokens":1,"metadata":{"sample":"sample"},"object":"realtime.response","output":[{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}],"output_modalities":["text"],"status":"completed","status_details":{"error":{"code":"sample","type":"sample"},"reason":"turn_detected","type":"completed"},"usage":{"input_token_details":{"audio_tokens":1,"cached_tokens":1,"cached_tokens_details":{"audio_tokens":1,"image_tokens":1,"text_tokens":1},"image_tokens":1,"text_tokens":1},"input_tokens":1,"output_token_details":{"audio_tokens":1,"text_tokens":1},"output_tokens":1,"total_tokens":1}},"type":"response.created"}"#,
        &["event_id", "response", "type"],
        &[],
    );
    object::<ResponseDoneEvent>(
        r#"{"event_id":"sample","response":{"id":"sample","audio":{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}},"conversation_id":"sample","max_output_tokens":1,"metadata":{"sample":"sample"},"object":"realtime.response","output":[{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}],"output_modalities":["text"],"status":"completed","status_details":{"error":{"code":"sample","type":"sample"},"reason":"turn_detected","type":"completed"},"usage":{"input_token_details":{"audio_tokens":1,"cached_tokens":1,"cached_tokens_details":{"audio_tokens":1,"image_tokens":1,"text_tokens":1},"image_tokens":1,"text_tokens":1},"input_tokens":1,"output_token_details":{"audio_tokens":1,"text_tokens":1},"output_tokens":1,"total_tokens":1}},"type":"response.done"}"#,
        &["event_id", "response", "type"],
        &[],
    );
    object::<ResponseFunctionCallArgumentsDeltaEvent>(
        r#"{"call_id":"sample","delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.function_call_arguments.delta"}"#,
        &[
            "call_id",
            "delta",
            "event_id",
            "item_id",
            "output_index",
            "response_id",
            "type",
        ],
        &[],
    );
    object::<ResponseFunctionCallArgumentsDoneEvent>(
        r#"{"arguments":"sample","call_id":"sample","event_id":"sample","item_id":"sample","name":"sample","output_index":1,"response_id":"sample","type":"response.function_call_arguments.done"}"#,
        &[
            "arguments",
            "call_id",
            "event_id",
            "item_id",
            "name",
            "output_index",
            "response_id",
            "type",
        ],
        &[],
    );
    object::<ResponseOutputItemAddedEvent>(
        r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"output_index":1,"response_id":"sample","type":"response.output_item.added"}"#,
        &["event_id", "item", "output_index", "response_id", "type"],
        &[],
    );
    object::<ResponseOutputItemDoneEvent>(
        r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"output_index":1,"response_id":"sample","type":"response.output_item.done"}"#,
        &["event_id", "item", "output_index", "response_id", "type"],
        &[],
    );
    object::<ResponseTextDeltaEvent>(
        r#"{"content_index":1,"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_text.delta"}"#,
        &[
            "content_index",
            "delta",
            "event_id",
            "item_id",
            "output_index",
            "response_id",
            "type",
        ],
        &[],
    );
    object::<ResponseTextDoneEvent>(
        r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","text":"sample","type":"response.output_text.done"}"#,
        &[
            "content_index",
            "event_id",
            "item_id",
            "output_index",
            "response_id",
            "text",
            "type",
        ],
        &[],
    );
    object::<SessionCreatedEvent>(
        r#"{"event_id":"sample","session":{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"},"type":"session.created"}"#,
        &["event_id", "session", "type"],
        &[],
    );
    object::<SessionUpdatedEvent>(
        r#"{"event_id":"sample","session":{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"},"type":"session.updated"}"#,
        &["event_id", "session", "type"],
        &[],
    );
    object::<RealtimeServerEventOutputAudioBufferStarted>(
        r#"{"event_id":"sample","response_id":"sample","type":"output_audio_buffer.started"}"#,
        &["event_id", "response_id", "type"],
        &[],
    );
    object::<RealtimeServerEventOutputAudioBufferStopped>(
        r#"{"event_id":"sample","response_id":"sample","type":"output_audio_buffer.stopped"}"#,
        &["event_id", "response_id", "type"],
        &[],
    );
    object::<RealtimeServerEventOutputAudioBufferCleared>(
        r#"{"event_id":"sample","response_id":"sample","type":"output_audio_buffer.cleared"}"#,
        &["event_id", "response_id", "type"],
        &[],
    );
    object::<ConversationItemAdded>(
        r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.added","previous_item_id":"sample"}"#,
        &["event_id", "item", "type"],
        &["previous_item_id"],
    );
    object::<ConversationItemDone>(
        r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.done","previous_item_id":"sample"}"#,
        &["event_id", "item", "type"],
        &["previous_item_id"],
    );
    object::<InputAudioBufferTimeoutTriggered>(
        r#"{"audio_end_ms":1,"audio_start_ms":1,"event_id":"sample","item_id":"sample","type":"input_audio_buffer.timeout_triggered"}"#,
        &[
            "audio_end_ms",
            "audio_start_ms",
            "event_id",
            "item_id",
            "type",
        ],
        &[],
    );
    object::<ConversationItemInputAudioTranscriptionSegment>(
        r#"{"id":"sample","content_index":1,"end":0.5,"event_id":"sample","item_id":"sample","speaker":"sample","start":0.5,"text":"sample","type":"conversation.item.input_audio_transcription.segment"}"#,
        &[
            "id",
            "content_index",
            "end",
            "event_id",
            "item_id",
            "speaker",
            "start",
            "text",
            "type",
        ],
        &[],
    );
    object::<McpListToolsInProgress>(
        r#"{"event_id":"sample","item_id":"sample","type":"mcp_list_tools.in_progress"}"#,
        &["event_id", "item_id", "type"],
        &[],
    );
    object::<McpListToolsCompleted>(
        r#"{"event_id":"sample","item_id":"sample","type":"mcp_list_tools.completed"}"#,
        &["event_id", "item_id", "type"],
        &[],
    );
    object::<McpListToolsFailed>(
        r#"{"event_id":"sample","item_id":"sample","type":"mcp_list_tools.failed"}"#,
        &["event_id", "item_id", "type"],
        &[],
    );
    object::<ResponseMcpCallArgumentsDelta>(
        r#"{"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.mcp_call_arguments.delta","obfuscation":"sample"}"#,
        &[
            "delta",
            "event_id",
            "item_id",
            "output_index",
            "response_id",
            "type",
        ],
        &["obfuscation"],
    );
    object::<ResponseMcpCallArgumentsDone>(
        r#"{"arguments":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.mcp_call_arguments.done"}"#,
        &[
            "arguments",
            "event_id",
            "item_id",
            "output_index",
            "response_id",
            "type",
        ],
        &[],
    );
    object::<ResponseMcpCallInProgress>(
        r#"{"event_id":"sample","item_id":"sample","output_index":1,"type":"response.mcp_call.in_progress"}"#,
        &["event_id", "item_id", "output_index", "type"],
        &[],
    );
    object::<ResponseMcpCallCompleted>(
        r#"{"event_id":"sample","item_id":"sample","output_index":1,"type":"response.mcp_call.completed"}"#,
        &["event_id", "item_id", "output_index", "type"],
        &[],
    );
    object::<ResponseMcpCallFailed>(
        r#"{"event_id":"sample","item_id":"sample","output_index":1,"type":"response.mcp_call.failed"}"#,
        &["event_id", "item_id", "output_index", "type"],
        &[],
    );
    object::<RealtimeAudioConfig>(
        r#"{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}}"#,
        &[],
        &["input", "output"],
    );
    object::<ResponsePrompt>(
        r#"{"id":"sample","variables":{"sample":"sample"},"version":"sample"}"#,
        &["id"],
        &["variables", "version"],
    );
    object::<RealtimeReasoning>(r#"{"effort":"minimal"}"#, &[], &["effort"]);
    object::<RealtimeSessionCreateResponseAudio>(
        r#"{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}}"#,
        &[],
        &["input", "output"],
    );
    object::<RealtimeTranscriptionSessionCreateResponseAudio>(
        r#"{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5,"type":"sample"}}}"#,
        &[],
        &["input"],
    );
    object::<RealtimeResponseCreateParams>(
        r#"{"audio":{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}},"conversation":"sample","input":[{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}],"instructions":"sample","max_output_tokens":1,"metadata":{"sample":"sample"},"output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}]}"#,
        &[],
        &[
            "audio",
            "conversation",
            "input",
            "instructions",
            "max_output_tokens",
            "metadata",
            "output_modalities",
            "parallel_tool_calls",
            "prompt",
            "reasoning",
            "tool_choice",
            "tools",
        ],
    );
    object::<ConversationCreatedEventConversation>(
        r#"{"id":"sample","object":"realtime.conversation"}"#,
        &[],
        &["id", "object"],
    );
    object::<TranscriptionLanguage>(r#"{"code":"sample"}"#, &["code"], &[]);
    object::<LogProbProperties>(
        r#"{"token":"sample","bytes":[1],"logprob":0.5}"#,
        &["token", "bytes", "logprob"],
        &[],
    );
    object::<ConversationItemInputAudioTranscriptionFailedEventError>(
        r#"{"code":"sample","message":"sample","param":"sample","type":"sample"}"#,
        &[],
        &["code", "message", "param", "type"],
    );
    object::<RealtimeError>(
        r#"{"message":"sample","type":"sample","code":"sample","event_id":"sample","param":"sample"}"#,
        &["message", "type"],
        &["code", "event_id", "param"],
    );
    object::<RateLimitsUpdatedEventRateLimit>(
        r#"{"limit":1,"name":"requests","remaining":1,"reset_seconds":0.5}"#,
        &[],
        &["limit", "name", "remaining", "reset_seconds"],
    );
    object::<ResponseContentPartAddedEventPart>(
        r#"{"audio":"sample","text":"sample","transcript":"sample","type":"text"}"#,
        &[],
        &["audio", "text", "transcript", "type"],
    );
    object::<ResponseContentPartDoneEventPart>(
        r#"{"audio":"sample","text":"sample","transcript":"sample","type":"text"}"#,
        &[],
        &["audio", "text", "transcript", "type"],
    );
    object::<RealtimeResponse>(
        r#"{"id":"sample","audio":{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}},"conversation_id":"sample","max_output_tokens":1,"metadata":{"sample":"sample"},"object":"realtime.response","output":[{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}],"output_modalities":["text"],"status":"completed","status_details":{"error":{"code":"sample","type":"sample"},"reason":"turn_detected","type":"completed"},"usage":{"input_token_details":{"audio_tokens":1,"cached_tokens":1,"cached_tokens_details":{"audio_tokens":1,"image_tokens":1,"text_tokens":1},"image_tokens":1,"text_tokens":1},"input_tokens":1,"output_token_details":{"audio_tokens":1,"text_tokens":1},"output_tokens":1,"total_tokens":1}}"#,
        &[],
        &[
            "id",
            "audio",
            "conversation_id",
            "max_output_tokens",
            "metadata",
            "object",
            "output",
            "output_modalities",
            "status",
            "status_details",
            "usage",
        ],
    );
    object::<RealtimeAudioConfigInput>(
        r#"{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}}"#,
        &[],
        &[
            "format",
            "noise_reduction",
            "transcription",
            "turn_detection",
        ],
    );
    object::<RealtimeAudioConfigOutput>(
        r#"{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}"#,
        &[],
        &["format", "speed", "voice"],
    );
    object::<ToolChoiceFunction>(
        r#"{"name":"sample","type":"function"}"#,
        &["name", "type"],
        &[],
    );
    object::<ToolChoiceMcp>(
        r#"{"server_label":"sample","type":"mcp","name":"sample"}"#,
        &["server_label", "type"],
        &["name"],
    );
    object::<RealtimeTracingConfigTracingConfiguration>(
        r#"{"group_id":"sample","metadata":{"opaque":true},"workflow_name":"sample"}"#,
        &[],
        &["group_id", "metadata", "workflow_name"],
    );
    object::<RealtimeTruncationRetentionRatio>(
        r#"{"retention_ratio":0.5,"type":"retention_ratio","token_limits":{"post_instructions":1}}"#,
        &["retention_ratio", "type"],
        &["token_limits"],
    );
    object::<RealtimeSessionCreateResponseAudioInput>(
        r#"{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}}"#,
        &[],
        &[
            "format",
            "noise_reduction",
            "transcription",
            "turn_detection",
        ],
    );
    object::<RealtimeSessionCreateResponseAudioOutput>(
        r#"{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}"#,
        &[],
        &["format", "speed", "voice"],
    );
    object::<RealtimeFunctionTool>(
        r#"{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}"#,
        &[],
        &["description", "name", "parameters", "type"],
    );
    object::<RealtimeSessionCreateResponseToolMcpTool>(
        r#"{"server_label":"sample","type":"mcp","allowed_callers":["direct"],"allowed_tools":["sample"],"authorization":"sample","connector_id":"connector_dropbox","defer_loading":true,"headers":{"sample":"sample"},"require_approval":{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}},"server_description":"sample","server_url":"sample","tunnel_id":"sample"}"#,
        &["server_label", "type"],
        &[
            "allowed_callers",
            "allowed_tools",
            "authorization",
            "connector_id",
            "defer_loading",
            "headers",
            "require_approval",
            "server_description",
            "server_url",
            "tunnel_id",
        ],
    );
    object::<RealtimeSessionCreateResponseTracingTracingConfiguration>(
        r#"{"group_id":"sample","metadata":{"opaque":true},"workflow_name":"sample"}"#,
        &[],
        &["group_id", "metadata", "workflow_name"],
    );
    object::<RealtimeTranscriptionSessionCreateResponseAudioInput>(
        r#"{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5,"type":"sample"}}"#,
        &[],
        &[
            "format",
            "noise_reduction",
            "transcription",
            "turn_detection",
        ],
    );
    object::<RealtimeConversationItemSystemMessage>(
        r#"{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}"#,
        &["content", "role", "type"],
        &["id", "object", "status"],
    );
    object::<RealtimeConversationItemUserMessage>(
        r#"{"content":[{"audio":"sample","detail":"auto","image_url":"sample","text":"sample","transcript":"sample","type":"input_text"}],"role":"user","type":"message","id":"sample","object":"realtime.item","status":"completed"}"#,
        &["content", "role", "type"],
        &["id", "object", "status"],
    );
    object::<RealtimeConversationItemAssistantMessage>(
        r#"{"content":[{"audio":"sample","text":"sample","transcript":"sample","type":"output_text"}],"role":"assistant","type":"message","id":"sample","object":"realtime.item","status":"completed"}"#,
        &["content", "role", "type"],
        &["id", "object", "status"],
    );
    object::<RealtimeConversationItemFunctionCall>(
        r#"{"arguments":"sample","name":"sample","type":"function_call","id":"sample","call_id":"sample","object":"realtime.item","status":"completed"}"#,
        &["arguments", "name", "type"],
        &["id", "call_id", "object", "status"],
    );
    object::<RealtimeConversationItemFunctionCallOutput>(
        r#"{"call_id":"sample","output":"sample","type":"function_call_output","id":"sample","object":"realtime.item","status":"completed"}"#,
        &["call_id", "output", "type"],
        &["id", "object", "status"],
    );
    object::<RealtimeMcpApprovalResponse>(
        r#"{"id":"sample","approval_request_id":"sample","approve":true,"type":"mcp_approval_response","reason":"sample"}"#,
        &["id", "approval_request_id", "approve", "type"],
        &["reason"],
    );
    object::<RealtimeMcpListTools>(
        r#"{"server_label":"sample","tools":[{"input_schema":{"opaque":true},"name":"sample","annotations":{"opaque":true},"description":"sample"}],"type":"mcp_list_tools","id":"sample"}"#,
        &["server_label", "tools", "type"],
        &["id"],
    );
    object::<RealtimeMcpToolCall>(
        r#"{"id":"sample","arguments":"sample","name":"sample","server_label":"sample","type":"mcp_call","approval_request_id":"sample","error":{"code":1,"message":"sample","type":"protocol_error"},"output":"sample"}"#,
        &["id", "arguments", "name", "server_label", "type"],
        &["approval_request_id", "error", "output"],
    );
    object::<RealtimeMcpApprovalRequest>(
        r#"{"id":"sample","arguments":"sample","name":"sample","server_label":"sample","type":"mcp_approval_request"}"#,
        &["id", "arguments", "name", "server_label", "type"],
        &[],
    );
    object::<RealtimeResponseCreateAudioOutput>(
        r#"{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}}"#,
        &[],
        &["output"],
    );
    object::<RealtimeTranscriptionSessionCreateRequest>(
        r#"{"type":"transcription","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}}},"include":["item.input_audio_transcription.logprobs"]}"#,
        &["type"],
        &["audio", "include"],
    );
    object::<ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageTokens>(
        r#"{"input_tokens":1,"output_tokens":1,"total_tokens":1,"type":"tokens","input_token_details":{"audio_tokens":1,"text_tokens":1}}"#,
        &["input_tokens", "output_tokens", "total_tokens", "type"],
        &["input_token_details"],
    );
    object::<ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageDuration>(
        r#"{"seconds":0.5,"type":"duration"}"#,
        &["seconds", "type"],
        &[],
    );
    object::<RealtimeResponseAudio>(
        r#"{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}}"#,
        &[],
        &["output"],
    );
    object::<RealtimeResponseStatus>(
        r#"{"error":{"code":"sample","type":"sample"},"reason":"turn_detected","type":"completed"}"#,
        &[],
        &["error", "reason", "type"],
    );
    object::<RealtimeResponseUsage>(
        r#"{"input_token_details":{"audio_tokens":1,"cached_tokens":1,"cached_tokens_details":{"audio_tokens":1,"image_tokens":1,"text_tokens":1},"image_tokens":1,"text_tokens":1},"input_tokens":1,"output_token_details":{"audio_tokens":1,"text_tokens":1},"output_tokens":1,"total_tokens":1}"#,
        &[],
        &[
            "input_token_details",
            "input_tokens",
            "output_token_details",
            "output_tokens",
            "total_tokens",
        ],
    );
    object::<RealtimeAudioConfigInputNoiseReduction>(r#"{"type":"near_field"}"#, &[], &["type"]);
    object::<AudioTranscription>(
        r#"{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"}"#,
        &[],
        &[
            "delay",
            "keywords",
            "language",
            "languages",
            "model",
            "prompt",
        ],
    );
    object::<ResponseInputText>(
        r#"{"text":"sample","type":"input_text","prompt_cache_breakpoint":{"mode":"explicit"}}"#,
        &["text", "type"],
        &["prompt_cache_breakpoint"],
    );
    object::<ResponseInputImage>(
        r#"{"detail":"low","type":"input_image","file_id":"sample","image_url":"sample","prompt_cache_breakpoint":{"mode":"explicit"}}"#,
        &["detail", "type"],
        &["file_id", "image_url", "prompt_cache_breakpoint"],
    );
    object::<ResponseInputFile>(
        r#"{"type":"input_file","detail":"auto","file_data":"sample","file_id":"sample","file_url":"sample","filename":"sample","prompt_cache_breakpoint":{"mode":"explicit"}}"#,
        &["type"],
        &[
            "detail",
            "file_data",
            "file_id",
            "file_url",
            "filename",
            "prompt_cache_breakpoint",
        ],
    );
    object::<RealtimeToolsConfigUnionMcp>(
        r#"{"server_label":"sample","type":"mcp","allowed_callers":["direct"],"allowed_tools":["sample"],"authorization":"sample","connector_id":"connector_dropbox","defer_loading":true,"headers":{"sample":"sample"},"require_approval":{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}},"server_description":"sample","server_url":"sample","tunnel_id":"sample"}"#,
        &["server_label", "type"],
        &[
            "allowed_callers",
            "allowed_tools",
            "authorization",
            "connector_id",
            "defer_loading",
            "headers",
            "require_approval",
            "server_description",
            "server_url",
            "tunnel_id",
        ],
    );
    object::<RealtimeTruncationRetentionRatioTokenLimits>(
        r#"{"post_instructions":1}"#,
        &[],
        &["post_instructions"],
    );
    object::<RealtimeSessionCreateResponseAudioInputNoiseReduction>(
        r#"{"type":"near_field"}"#,
        &[],
        &["type"],
    );
    object::<RealtimeTranscriptionSessionCreateResponseAudioInputNoiseReduction>(
        r#"{"type":"near_field"}"#,
        &[],
        &["type"],
    );
    object::<RealtimeTranscriptionSessionTurnDetection>(
        r#"{"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5,"type":"sample"}"#,
        &[],
        &[
            "prefix_padding_ms",
            "silence_duration_ms",
            "threshold",
            "type",
        ],
    );
    object::<RealtimeConversationItemSystemMessageContent>(
        r#"{"text":"sample","type":"input_text"}"#,
        &[],
        &["text", "type"],
    );
    object::<RealtimeConversationItemUserMessageContent>(
        r#"{"audio":"sample","detail":"auto","image_url":"sample","text":"sample","transcript":"sample","type":"input_text"}"#,
        &[],
        &["audio", "detail", "image_url", "text", "transcript", "type"],
    );
    object::<RealtimeConversationItemAssistantMessageContent>(
        r#"{"audio":"sample","text":"sample","transcript":"sample","type":"output_text"}"#,
        &[],
        &["audio", "text", "transcript", "type"],
    );
    object::<RealtimeMcpListToolsTool>(
        r#"{"input_schema":{"opaque":true},"name":"sample","annotations":{"opaque":true},"description":"sample"}"#,
        &["input_schema", "name"],
        &["annotations", "description"],
    );
    object::<RealtimeResponseCreateAudioOutputOutput>(
        r#"{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}"#,
        &[],
        &["format", "voice"],
    );
    object::<RealtimeResponseCreateMcpTool>(
        r#"{"server_label":"sample","type":"mcp","allowed_callers":["direct"],"allowed_tools":["sample"],"authorization":"sample","connector_id":"connector_dropbox","defer_loading":true,"headers":{"sample":"sample"},"require_approval":{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}},"server_description":"sample","server_url":"sample","tunnel_id":"sample"}"#,
        &["server_label", "type"],
        &[
            "allowed_callers",
            "allowed_tools",
            "authorization",
            "connector_id",
            "defer_loading",
            "headers",
            "require_approval",
            "server_description",
            "server_url",
            "tunnel_id",
        ],
    );
    object::<RealtimeTranscriptionSessionAudio>(
        r#"{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}}}"#,
        &[],
        &["input"],
    );
    object::<ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageTokensInputTokenDetails>(r#"{"audio_tokens":1,"text_tokens":1}"#, &[], &["audio_tokens","text_tokens"]);
    object::<RealtimeResponseAudioOutput>(
        r#"{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}"#,
        &[],
        &["format", "voice"],
    );
    object::<RealtimeResponseStatusError>(
        r#"{"code":"sample","type":"sample"}"#,
        &[],
        &["code", "type"],
    );
    object::<RealtimeResponseUsageInputTokenDetails>(
        r#"{"audio_tokens":1,"cached_tokens":1,"cached_tokens_details":{"audio_tokens":1,"image_tokens":1,"text_tokens":1},"image_tokens":1,"text_tokens":1}"#,
        &[],
        &[
            "audio_tokens",
            "cached_tokens",
            "cached_tokens_details",
            "image_tokens",
            "text_tokens",
        ],
    );
    object::<RealtimeResponseUsageOutputTokenDetails>(
        r#"{"audio_tokens":1,"text_tokens":1}"#,
        &[],
        &["audio_tokens", "text_tokens"],
    );
    object::<RealtimeAudioFormatsAudioPCM>(
        r#"{"rate":24000,"type":"audio/pcm"}"#,
        &[],
        &["rate", "type"],
    );
    object::<RealtimeAudioFormatsAudioPCMU>(r#"{"type":"audio/pcmu"}"#, &[], &["type"]);
    object::<RealtimeAudioFormatsAudioPCMA>(r#"{"type":"audio/pcma"}"#, &[], &["type"]);
    object::<RealtimeAudioInputTurnDetectionServerVad>(
        r#"{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}"#,
        &["type"],
        &[
            "create_response",
            "idle_timeout_ms",
            "interrupt_response",
            "prefix_padding_ms",
            "silence_duration_ms",
            "threshold",
        ],
    );
    object::<RealtimeAudioInputTurnDetectionSemanticVad>(
        r#"{"type":"semantic_vad","create_response":true,"eagerness":"low","interrupt_response":true}"#,
        &["type"],
        &["create_response", "eagerness", "interrupt_response"],
    );
    object::<RealtimeAudioConfigOutputVoiceID>(r#"{"id":"sample"}"#, &["id"], &[]);
    object::<ResponseInputTextPromptCacheBreakpoint>(r#"{"mode":"explicit"}"#, &["mode"], &[]);
    object::<ResponseInputImagePromptCacheBreakpoint>(r#"{"mode":"explicit"}"#, &["mode"], &[]);
    object::<ResponseInputFilePromptCacheBreakpoint>(r#"{"mode":"explicit"}"#, &["mode"], &[]);
    object::<RealtimeSessionCreateResponseAudioInputTurnDetectionServerVad>(
        r#"{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}"#,
        &["type"],
        &[
            "create_response",
            "idle_timeout_ms",
            "interrupt_response",
            "prefix_padding_ms",
            "silence_duration_ms",
            "threshold",
        ],
    );
    object::<RealtimeSessionCreateResponseAudioInputTurnDetectionSemanticVad>(
        r#"{"type":"semantic_vad","create_response":true,"eagerness":"low","interrupt_response":true}"#,
        &["type"],
        &["create_response", "eagerness", "interrupt_response"],
    );
    object::<RealtimeSessionCreateResponseToolMcpToolAllowedToolsMcpToolFilter>(
        r#"{"read_only":true,"tool_names":["sample"]}"#,
        &[],
        &["read_only", "tool_names"],
    );
    object::<RealtimeSessionCreateResponseToolMcpToolRequireApprovalMcpToolApprovalFilter>(
        r#"{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}}"#,
        &[],
        &["always", "never"],
    );
    object::<RealtimeMcpProtocolError>(
        r#"{"code":1,"message":"sample","type":"protocol_error"}"#,
        &["code", "message", "type"],
        &[],
    );
    object::<RealtimeMcpToolExecutionError>(
        r#"{"message":"sample","type":"tool_execution_error"}"#,
        &["message", "type"],
        &[],
    );
    object::<RealtimeMcphttpError>(
        r#"{"code":1,"message":"sample","type":"http_error"}"#,
        &["code", "message", "type"],
        &[],
    );
    object::<RealtimeTranscriptionSessionAudioInput>(
        r#"{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}}"#,
        &[],
        &[
            "format",
            "noise_reduction",
            "transcription",
            "turn_detection",
        ],
    );
    object::<RealtimeResponseUsageInputTokenDetailsCachedTokensDetails>(
        r#"{"audio_tokens":1,"image_tokens":1,"text_tokens":1}"#,
        &[],
        &["audio_tokens", "image_tokens", "text_tokens"],
    );
    object::<RealtimeToolsConfigUnionMcpAllowedToolsMcpToolFilter>(
        r#"{"read_only":true,"tool_names":["sample"]}"#,
        &[],
        &["read_only", "tool_names"],
    );
    object::<RealtimeToolsConfigUnionMcpRequireApprovalMcpToolApprovalFilter>(
        r#"{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}}"#,
        &[],
        &["always", "never"],
    );
    object::<RealtimeSessionCreateResponseToolMcpToolRequireApprovalMcpToolApprovalFilterAlways>(
        r#"{"read_only":true,"tool_names":["sample"]}"#,
        &[],
        &["read_only", "tool_names"],
    );
    object::<RealtimeSessionCreateResponseToolMcpToolRequireApprovalMcpToolApprovalFilterNever>(
        r#"{"read_only":true,"tool_names":["sample"]}"#,
        &[],
        &["read_only", "tool_names"],
    );
    object::<RealtimeResponseCreateAudioOutputOutputVoiceID>(r#"{"id":"sample"}"#, &["id"], &[]);
    object::<RealtimeResponseCreateMcpToolAllowedToolsMcpToolFilter>(
        r#"{"read_only":true,"tool_names":["sample"]}"#,
        &[],
        &["read_only", "tool_names"],
    );
    object::<RealtimeResponseCreateMcpToolRequireApprovalMcpToolApprovalFilter>(
        r#"{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}}"#,
        &[],
        &["always", "never"],
    );
    object::<RealtimeTranscriptionSessionAudioInputNoiseReduction>(
        r#"{"type":"near_field"}"#,
        &[],
        &["type"],
    );
    object::<RealtimeToolsConfigUnionMcpRequireApprovalMcpToolApprovalFilterAlways>(
        r#"{"read_only":true,"tool_names":["sample"]}"#,
        &[],
        &["read_only", "tool_names"],
    );
    object::<RealtimeToolsConfigUnionMcpRequireApprovalMcpToolApprovalFilterNever>(
        r#"{"read_only":true,"tool_names":["sample"]}"#,
        &[],
        &["read_only", "tool_names"],
    );
    object::<RealtimeResponseCreateMcpToolRequireApprovalMcpToolApprovalFilterAlways>(
        r#"{"read_only":true,"tool_names":["sample"]}"#,
        &[],
        &["read_only", "tool_names"],
    );
    object::<RealtimeResponseCreateMcpToolRequireApprovalMcpToolApprovalFilterNever>(
        r#"{"read_only":true,"tool_names":["sample"]}"#,
        &[],
        &["read_only", "tool_names"],
    );
    object::<RealtimeTranscriptionSessionAudioInputTurnDetectionServerVad>(
        r#"{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}"#,
        &["type"],
        &[
            "create_response",
            "idle_timeout_ms",
            "interrupt_response",
            "prefix_padding_ms",
            "silence_duration_ms",
            "threshold",
        ],
    );
    object::<RealtimeTranscriptionSessionAudioInputTurnDetectionSemanticVad>(
        r#"{"type":"semantic_vad","create_response":true,"eagerness":"low","interrupt_response":true}"#,
        &["type"],
        &["create_response", "eagerness", "interrupt_response"],
    );
    object::<ConversationItemWithReference>(
        r#"{"id":"sample","arguments":"sample","call_id":"sample","content":[{"id":"sample","audio":"sample","text":"sample","transcript":"sample","type":"input_audio"}],"name":"sample","object":"realtime.item","output":"sample","role":"user","status":"completed","type":"message"}"#,
        &[],
        &[],
    );
    object::<ConversationItemWithReferenceContent>(
        r#"{"id":"sample","audio":"sample","text":"sample","transcript":"sample","type":"input_audio"}"#,
        &[],
        &[],
    );
}
#[test]
fn every_upstream_union_branch_round_trips() {
    round_trip::<RealtimeClientEvent>(serde_json::from_str(r#"{"item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.create","event_id":"sample","previous_item_id":"sample"}"#).unwrap(),true);
    round_trip::<RealtimeClientEvent>(
        serde_json::from_str(
            r#"{"item_id":"sample","type":"conversation.item.delete","event_id":"sample"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeClientEvent>(
        serde_json::from_str(
            r#"{"item_id":"sample","type":"conversation.item.retrieve","event_id":"sample"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeClientEvent>(serde_json::from_str(r#"{"audio_end_ms":1,"content_index":1,"item_id":"sample","type":"conversation.item.truncate","event_id":"sample"}"#).unwrap(),true);
    round_trip::<RealtimeClientEvent>(
        serde_json::from_str(
            r#"{"audio":"sample","type":"input_audio_buffer.append","event_id":"sample"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeClientEvent>(
        serde_json::from_str(r#"{"type":"input_audio_buffer.clear","event_id":"sample"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeClientEvent>(
        serde_json::from_str(r#"{"type":"output_audio_buffer.clear","event_id":"sample"}"#)
            .unwrap(),
        true,
    );
    round_trip::<RealtimeClientEvent>(
        serde_json::from_str(r#"{"type":"input_audio_buffer.commit","event_id":"sample"}"#)
            .unwrap(),
        true,
    );
    round_trip::<RealtimeClientEvent>(
        serde_json::from_str(
            r#"{"type":"response.cancel","event_id":"sample","response_id":"sample"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeClientEvent>(serde_json::from_str(r#"{"type":"response.create","event_id":"sample","response":{"audio":{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}},"conversation":"sample","input":[{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}],"instructions":"sample","max_output_tokens":1,"metadata":{"sample":"sample"},"output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}]}}"#).unwrap(),true);
    round_trip::<RealtimeClientEvent>(serde_json::from_str(r#"{"session":{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"},"type":"session.update","event_id":"sample"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"conversation":{"id":"sample","object":"realtime.conversation"},"event_id":"sample","type":"conversation.created"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.created","previous_item_id":"sample"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(
        serde_json::from_str(
            r#"{"event_id":"sample","item_id":"sample","type":"conversation.item.deleted"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","transcript":"sample","type":"conversation.item.input_audio_transcription.completed","usage":{"input_tokens":1,"output_tokens":1,"total_tokens":1,"type":"tokens","input_token_details":{"audio_tokens":1,"text_tokens":1}},"languages":[{"code":"sample"}],"logprobs":[{"token":"sample","bytes":[1],"logprob":0.5}]}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","item_id":"sample","type":"conversation.item.input_audio_transcription.delta","content_index":1,"delta":"sample","logprobs":[{"token":"sample","bytes":[1],"logprob":0.5}]}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"content_index":1,"error":{"code":"sample","message":"sample","param":"sample","type":"sample"},"event_id":"sample","item_id":"sample","type":"conversation.item.input_audio_transcription.failed"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.retrieved"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"audio_end_ms":1,"content_index":1,"event_id":"sample","item_id":"sample","type":"conversation.item.truncated"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"error":{"message":"sample","type":"sample","code":"sample","event_id":"sample","param":"sample"},"event_id":"sample","type":"error"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(
        serde_json::from_str(r#"{"event_id":"sample","type":"input_audio_buffer.cleared"}"#)
            .unwrap(),
        true,
    );
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","item_id":"sample","type":"input_audio_buffer.committed","previous_item_id":"sample"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(
        serde_json::from_str(
            r#"{"event":"sample","received_at":1,"type":"input_audio_buffer.dtmf_event_received"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"audio_start_ms":1,"event_id":"sample","item_id":"sample","type":"input_audio_buffer.speech_started"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"audio_end_ms":1,"event_id":"sample","item_id":"sample","type":"input_audio_buffer.speech_stopped"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","rate_limits":[{"limit":1,"name":"requests","remaining":1,"reset_seconds":0.5}],"type":"rate_limits.updated"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"content_index":1,"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_audio.delta"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_audio.done"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"content_index":1,"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_audio_transcript.delta"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","transcript":"sample","type":"response.output_audio_transcript.done"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"part":{"audio":"sample","text":"sample","transcript":"sample","type":"text"},"response_id":"sample","type":"response.content_part.added"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"part":{"audio":"sample","text":"sample","transcript":"sample","type":"text"},"response_id":"sample","type":"response.content_part.done"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","response":{"id":"sample","audio":{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}},"conversation_id":"sample","max_output_tokens":1,"metadata":{"sample":"sample"},"object":"realtime.response","output":[{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}],"output_modalities":["text"],"status":"completed","status_details":{"error":{"code":"sample","type":"sample"},"reason":"turn_detected","type":"completed"},"usage":{"input_token_details":{"audio_tokens":1,"cached_tokens":1,"cached_tokens_details":{"audio_tokens":1,"image_tokens":1,"text_tokens":1},"image_tokens":1,"text_tokens":1},"input_tokens":1,"output_token_details":{"audio_tokens":1,"text_tokens":1},"output_tokens":1,"total_tokens":1}},"type":"response.created"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","response":{"id":"sample","audio":{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}},"conversation_id":"sample","max_output_tokens":1,"metadata":{"sample":"sample"},"object":"realtime.response","output":[{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}],"output_modalities":["text"],"status":"completed","status_details":{"error":{"code":"sample","type":"sample"},"reason":"turn_detected","type":"completed"},"usage":{"input_token_details":{"audio_tokens":1,"cached_tokens":1,"cached_tokens_details":{"audio_tokens":1,"image_tokens":1,"text_tokens":1},"image_tokens":1,"text_tokens":1},"input_tokens":1,"output_token_details":{"audio_tokens":1,"text_tokens":1},"output_tokens":1,"total_tokens":1}},"type":"response.done"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"call_id":"sample","delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.function_call_arguments.delta"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"arguments":"sample","call_id":"sample","event_id":"sample","item_id":"sample","name":"sample","output_index":1,"response_id":"sample","type":"response.function_call_arguments.done"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"output_index":1,"response_id":"sample","type":"response.output_item.added"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"output_index":1,"response_id":"sample","type":"response.output_item.done"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"content_index":1,"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_text.delta"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","text":"sample","type":"response.output_text.done"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","session":{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"},"type":"session.created"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","session":{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"},"type":"session.updated"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(
        serde_json::from_str(
            r#"{"event_id":"sample","response_id":"sample","type":"output_audio_buffer.started"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeServerEvent>(
        serde_json::from_str(
            r#"{"event_id":"sample","response_id":"sample","type":"output_audio_buffer.stopped"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeServerEvent>(
        serde_json::from_str(
            r#"{"event_id":"sample","response_id":"sample","type":"output_audio_buffer.cleared"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.added","previous_item_id":"sample"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.done","previous_item_id":"sample"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"audio_end_ms":1,"audio_start_ms":1,"event_id":"sample","item_id":"sample","type":"input_audio_buffer.timeout_triggered"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"id":"sample","content_index":1,"end":0.5,"event_id":"sample","item_id":"sample","speaker":"sample","start":0.5,"text":"sample","type":"conversation.item.input_audio_transcription.segment"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(
        serde_json::from_str(
            r#"{"event_id":"sample","item_id":"sample","type":"mcp_list_tools.in_progress"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeServerEvent>(
        serde_json::from_str(
            r#"{"event_id":"sample","item_id":"sample","type":"mcp_list_tools.completed"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeServerEvent>(
        serde_json::from_str(
            r#"{"event_id":"sample","item_id":"sample","type":"mcp_list_tools.failed"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.mcp_call_arguments.delta","obfuscation":"sample"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"arguments":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.mcp_call_arguments.done"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","item_id":"sample","output_index":1,"type":"response.mcp_call.in_progress"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","item_id":"sample","output_index":1,"type":"response.mcp_call.completed"}"#).unwrap(),true);
    round_trip::<RealtimeServerEvent>(serde_json::from_str(r#"{"event_id":"sample","item_id":"sample","output_index":1,"type":"response.mcp_call.failed"}"#).unwrap(),true);
    round_trip::<RealtimeToolChoiceConfig>(serde_json::from_str(r#""none""#).unwrap(), true);
    round_trip::<RealtimeToolChoiceConfig>(
        serde_json::from_str(r#"{"name":"sample","type":"function"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeToolChoiceConfig>(
        serde_json::from_str(r#"{"server_label":"sample","type":"mcp","name":"sample"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeToolsConfig>(serde_json::from_str(r#"[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}]"#).unwrap(),true);
    round_trip::<RealtimeTracingConfig>(serde_json::from_str(r#""auto""#).unwrap(), true);
    round_trip::<RealtimeTracingConfig>(
        serde_json::from_str(
            r#"{"group_id":"sample","metadata":{"opaque":true},"workflow_name":"sample"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeTracingConfig>(serde_json::from_str(r#"null"#).unwrap(), true);
    round_trip::<RealtimeTruncation>(serde_json::from_str(r#""auto""#).unwrap(), true);
    round_trip::<RealtimeTruncation>(serde_json::from_str(r#""disabled""#).unwrap(), true);
    round_trip::<RealtimeTruncation>(serde_json::from_str(r#"{"retention_ratio":0.5,"type":"retention_ratio","token_limits":{"post_instructions":1}}"#).unwrap(),true);
    round_trip::<RealtimeSessionCreateResponseToolChoice>(
        serde_json::from_str(r#""none""#).unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseToolChoice>(
        serde_json::from_str(r#"{"name":"sample","type":"function"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseToolChoice>(
        serde_json::from_str(r#"{"server_label":"sample","type":"mcp","name":"sample"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseTool>(serde_json::from_str(r#"{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}"#).unwrap(),true);
    round_trip::<RealtimeSessionCreateResponseTool>(serde_json::from_str(r#"{"server_label":"sample","type":"mcp","allowed_callers":["direct"],"allowed_tools":["sample"],"authorization":"sample","connector_id":"connector_dropbox","defer_loading":true,"headers":{"sample":"sample"},"require_approval":{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}},"server_description":"sample","server_url":"sample","tunnel_id":"sample"}"#).unwrap(),true);
    round_trip::<RealtimeSessionCreateResponseTracing>(
        serde_json::from_str(r#""auto""#).unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseTracing>(
        serde_json::from_str(
            r#"{"group_id":"sample","metadata":{"opaque":true},"workflow_name":"sample"}"#,
        )
        .unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseTracing>(
        serde_json::from_str(r#"null"#).unwrap(),
        true,
    );
    round_trip::<ConversationItem>(serde_json::from_str(r#"{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}"#).unwrap(),true);
    round_trip::<ConversationItem>(serde_json::from_str(r#"{"content":[{"audio":"sample","detail":"auto","image_url":"sample","text":"sample","transcript":"sample","type":"input_text"}],"role":"user","type":"message","id":"sample","object":"realtime.item","status":"completed"}"#).unwrap(),true);
    round_trip::<ConversationItem>(serde_json::from_str(r#"{"content":[{"audio":"sample","text":"sample","transcript":"sample","type":"output_text"}],"role":"assistant","type":"message","id":"sample","object":"realtime.item","status":"completed"}"#).unwrap(),true);
    round_trip::<ConversationItem>(serde_json::from_str(r#"{"arguments":"sample","name":"sample","type":"function_call","id":"sample","call_id":"sample","object":"realtime.item","status":"completed"}"#).unwrap(),true);
    round_trip::<ConversationItem>(serde_json::from_str(r#"{"call_id":"sample","output":"sample","type":"function_call_output","id":"sample","object":"realtime.item","status":"completed"}"#).unwrap(),true);
    round_trip::<ConversationItem>(serde_json::from_str(r#"{"id":"sample","approval_request_id":"sample","approve":true,"type":"mcp_approval_response","reason":"sample"}"#).unwrap(),true);
    round_trip::<ConversationItem>(serde_json::from_str(r#"{"server_label":"sample","tools":[{"input_schema":{"opaque":true},"name":"sample","annotations":{"opaque":true},"description":"sample"}],"type":"mcp_list_tools","id":"sample"}"#).unwrap(),true);
    round_trip::<ConversationItem>(serde_json::from_str(r#"{"id":"sample","arguments":"sample","name":"sample","server_label":"sample","type":"mcp_call","approval_request_id":"sample","error":{"code":1,"message":"sample","type":"protocol_error"},"output":"sample"}"#).unwrap(),true);
    round_trip::<ConversationItem>(serde_json::from_str(r#"{"id":"sample","arguments":"sample","name":"sample","server_label":"sample","type":"mcp_approval_request"}"#).unwrap(),true);
    round_trip::<SessionUpdateEventSession>(serde_json::from_str(r#"{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"}"#).unwrap(),true);
    round_trip::<SessionUpdateEventSession>(serde_json::from_str(r#"{"type":"transcription","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}}},"include":["item.input_audio_transcription.logprobs"]}"#).unwrap(),true);
    round_trip::<ConversationItemInputAudioTranscriptionCompletedEventUsage>(serde_json::from_str(r#"{"input_tokens":1,"output_tokens":1,"total_tokens":1,"type":"tokens","input_token_details":{"audio_tokens":1,"text_tokens":1}}"#).unwrap(),true);
    round_trip::<ConversationItemInputAudioTranscriptionCompletedEventUsage>(
        serde_json::from_str(r#"{"seconds":0.5,"type":"duration"}"#).unwrap(),
        true,
    );
    round_trip::<SessionCreatedEventSession>(serde_json::from_str(r#"{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"}"#).unwrap(),true);
    round_trip::<SessionCreatedEventSession>(serde_json::from_str(r#"{"type":"transcription","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}}},"include":["item.input_audio_transcription.logprobs"]}"#).unwrap(),true);
    round_trip::<SessionUpdatedEventSession>(serde_json::from_str(r#"{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"}"#).unwrap(),true);
    round_trip::<SessionUpdatedEventSession>(serde_json::from_str(r#"{"type":"transcription","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}}},"include":["item.input_audio_transcription.logprobs"]}"#).unwrap(),true);
    round_trip::<ResponsePromptVariables>(serde_json::from_str(r#""sample""#).unwrap(), true);
    round_trip::<ResponsePromptVariables>(serde_json::from_str(r#"{"text":"sample","type":"input_text","prompt_cache_breakpoint":{"mode":"explicit"}}"#).unwrap(),true);
    round_trip::<ResponsePromptVariables>(serde_json::from_str(r#"{"detail":"low","type":"input_image","file_id":"sample","image_url":"sample","prompt_cache_breakpoint":{"mode":"explicit"}}"#).unwrap(),true);
    round_trip::<ResponsePromptVariables>(serde_json::from_str(r#"{"type":"input_file","detail":"auto","file_data":"sample","file_id":"sample","file_url":"sample","filename":"sample","prompt_cache_breakpoint":{"mode":"explicit"}}"#).unwrap(),true);
    round_trip::<RealtimeReasoningEffort>(serde_json::from_str(r#""minimal""#).unwrap(), true);
    round_trip::<RealtimeReasoningEffort>(serde_json::from_str(r#""low""#).unwrap(), true);
    round_trip::<RealtimeReasoningEffort>(serde_json::from_str(r#""medium""#).unwrap(), true);
    round_trip::<RealtimeReasoningEffort>(serde_json::from_str(r#""high""#).unwrap(), true);
    round_trip::<RealtimeReasoningEffort>(serde_json::from_str(r#""xhigh""#).unwrap(), true);
    round_trip::<ToolChoiceOptions>(serde_json::from_str(r#""none""#).unwrap(), true);
    round_trip::<ToolChoiceOptions>(serde_json::from_str(r#""auto""#).unwrap(), true);
    round_trip::<ToolChoiceOptions>(serde_json::from_str(r#""required""#).unwrap(), true);
    round_trip::<RealtimeToolsConfigUnion>(serde_json::from_str(r#"{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}"#).unwrap(),true);
    round_trip::<RealtimeToolsConfigUnion>(serde_json::from_str(r#"{"server_label":"sample","type":"mcp","allowed_callers":["direct"],"allowed_tools":["sample"],"authorization":"sample","connector_id":"connector_dropbox","defer_loading":true,"headers":{"sample":"sample"},"require_approval":{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}},"server_description":"sample","server_url":"sample","tunnel_id":"sample"}"#).unwrap(),true);
    round_trip::<Metadata>(
        serde_json::from_str(r#"{"sample":"sample"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateParamsToolChoice>(
        serde_json::from_str(r#""none""#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateParamsToolChoice>(
        serde_json::from_str(r#"{"name":"sample","type":"function"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateParamsToolChoice>(
        serde_json::from_str(r#"{"server_label":"sample","type":"mcp","name":"sample"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateParamsTool>(serde_json::from_str(r#"{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}"#).unwrap(),true);
    round_trip::<RealtimeResponseCreateParamsTool>(serde_json::from_str(r#"{"server_label":"sample","type":"mcp","allowed_callers":["direct"],"allowed_tools":["sample"],"authorization":"sample","connector_id":"connector_dropbox","defer_loading":true,"headers":{"sample":"sample"},"require_approval":{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}},"server_description":"sample","server_url":"sample","tunnel_id":"sample"}"#).unwrap(),true);
    round_trip::<RealtimeAudioFormats>(
        serde_json::from_str(r#"{"rate":24000,"type":"audio/pcm"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeAudioFormats>(
        serde_json::from_str(r#"{"type":"audio/pcmu"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeAudioFormats>(
        serde_json::from_str(r#"{"type":"audio/pcma"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeAudioInputTurnDetection>(serde_json::from_str(r#"{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}"#).unwrap(),true);
    round_trip::<RealtimeAudioInputTurnDetection>(serde_json::from_str(r#"{"type":"semantic_vad","create_response":true,"eagerness":"low","interrupt_response":true}"#).unwrap(),true);
    round_trip::<RealtimeAudioInputTurnDetection>(serde_json::from_str(r#"null"#).unwrap(), true);
    round_trip::<RealtimeAudioConfigOutputVoice>(
        serde_json::from_str(r#""sample""#).unwrap(),
        true,
    );
    round_trip::<RealtimeAudioConfigOutputVoice>(
        serde_json::from_str(r#"{"id":"sample"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseAudioInputTurnDetection>(serde_json::from_str(r#"{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}"#).unwrap(),true);
    round_trip::<RealtimeSessionCreateResponseAudioInputTurnDetection>(serde_json::from_str(r#"{"type":"semantic_vad","create_response":true,"eagerness":"low","interrupt_response":true}"#).unwrap(),true);
    round_trip::<RealtimeSessionCreateResponseAudioInputTurnDetection>(
        serde_json::from_str(r#"null"#).unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseToolMcpToolAllowedTools>(
        serde_json::from_str(r#"["sample"]"#).unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseToolMcpToolAllowedTools>(
        serde_json::from_str(r#"{"read_only":true,"tool_names":["sample"]}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseToolMcpToolAllowedTools>(
        serde_json::from_str(r#"null"#).unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseToolMcpToolRequireApproval>(serde_json::from_str(r#"{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}}"#).unwrap(),true);
    round_trip::<RealtimeSessionCreateResponseToolMcpToolRequireApproval>(
        serde_json::from_str(r#""always""#).unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseToolMcpToolRequireApproval>(
        serde_json::from_str(r#""never""#).unwrap(),
        true,
    );
    round_trip::<RealtimeSessionCreateResponseToolMcpToolRequireApproval>(
        serde_json::from_str(r#"null"#).unwrap(),
        true,
    );
    round_trip::<RealtimeMcpToolCallError>(
        serde_json::from_str(r#"{"code":1,"message":"sample","type":"protocol_error"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeMcpToolCallError>(
        serde_json::from_str(r#"{"message":"sample","type":"tool_execution_error"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeMcpToolCallError>(
        serde_json::from_str(r#"{"code":1,"message":"sample","type":"http_error"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeMcpToolCallError>(serde_json::from_str(r#"null"#).unwrap(), true);
    round_trip::<NoiseReductionType>(serde_json::from_str(r#""near_field""#).unwrap(), true);
    round_trip::<NoiseReductionType>(serde_json::from_str(r#""far_field""#).unwrap(), true);
    round_trip::<ImageDetail>(serde_json::from_str(r#""low""#).unwrap(), true);
    round_trip::<ImageDetail>(serde_json::from_str(r#""high""#).unwrap(), true);
    round_trip::<ImageDetail>(serde_json::from_str(r#""auto""#).unwrap(), true);
    round_trip::<ImageDetail>(serde_json::from_str(r#""original""#).unwrap(), true);
    round_trip::<RealtimeToolsConfigUnionMcpAllowedTools>(
        serde_json::from_str(r#"["sample"]"#).unwrap(),
        true,
    );
    round_trip::<RealtimeToolsConfigUnionMcpAllowedTools>(
        serde_json::from_str(r#"{"read_only":true,"tool_names":["sample"]}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeToolsConfigUnionMcpAllowedTools>(
        serde_json::from_str(r#"null"#).unwrap(),
        true,
    );
    round_trip::<RealtimeToolsConfigUnionMcpRequireApproval>(serde_json::from_str(r#"{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}}"#).unwrap(),true);
    round_trip::<RealtimeToolsConfigUnionMcpRequireApproval>(
        serde_json::from_str(r#""always""#).unwrap(),
        true,
    );
    round_trip::<RealtimeToolsConfigUnionMcpRequireApproval>(
        serde_json::from_str(r#""never""#).unwrap(),
        true,
    );
    round_trip::<RealtimeToolsConfigUnionMcpRequireApproval>(
        serde_json::from_str(r#"null"#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateAudioOutputOutputVoice>(
        serde_json::from_str(r#""sample""#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateAudioOutputOutputVoice>(
        serde_json::from_str(r#"{"id":"sample"}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateMcpToolAllowedTools>(
        serde_json::from_str(r#"["sample"]"#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateMcpToolAllowedTools>(
        serde_json::from_str(r#"{"read_only":true,"tool_names":["sample"]}"#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateMcpToolAllowedTools>(
        serde_json::from_str(r#"null"#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateMcpToolRequireApproval>(serde_json::from_str(r#"{"always":{"read_only":true,"tool_names":["sample"]},"never":{"read_only":true,"tool_names":["sample"]}}"#).unwrap(),true);
    round_trip::<RealtimeResponseCreateMcpToolRequireApproval>(
        serde_json::from_str(r#""always""#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateMcpToolRequireApproval>(
        serde_json::from_str(r#""never""#).unwrap(),
        true,
    );
    round_trip::<RealtimeResponseCreateMcpToolRequireApproval>(
        serde_json::from_str(r#"null"#).unwrap(),
        true,
    );
    round_trip::<RealtimeTranscriptionSessionAudioInputTurnDetection>(serde_json::from_str(r#"{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}"#).unwrap(),true);
    round_trip::<RealtimeTranscriptionSessionAudioInputTurnDetection>(serde_json::from_str(r#"{"type":"semantic_vad","create_response":true,"eagerness":"low","interrupt_response":true}"#).unwrap(),true);
    round_trip::<RealtimeTranscriptionSessionAudioInputTurnDetection>(
        serde_json::from_str(r#"null"#).unwrap(),
        true,
    );
}
#[test]
fn all_11_client_and_46_server_events_select_their_native_variant() {
    {
        let value:Value=serde_json::from_str(r#"{"item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.create","event_id":"sample","previous_item_id":"sample"}"#).unwrap();
        let event: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeClientEvent::ConversationItemCreateEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"item_id":"sample","type":"conversation.item.delete","event_id":"sample"}"#,
        )
        .unwrap();
        let event: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeClientEvent::ConversationItemDeleteEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"item_id":"sample","type":"conversation.item.retrieve","event_id":"sample"}"#,
        )
        .unwrap();
        let event: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeClientEvent::ConversationItemRetrieveEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"audio_end_ms":1,"content_index":1,"item_id":"sample","type":"conversation.item.truncate","event_id":"sample"}"#).unwrap();
        let event: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeClientEvent::ConversationItemTruncateEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"audio":"sample","type":"input_audio_buffer.append","event_id":"sample"}"#,
        )
        .unwrap();
        let event: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeClientEvent::InputAudioBufferAppendEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value =
            serde_json::from_str(r#"{"type":"input_audio_buffer.clear","event_id":"sample"}"#)
                .unwrap();
        let event: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeClientEvent::InputAudioBufferClearEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value =
            serde_json::from_str(r#"{"type":"output_audio_buffer.clear","event_id":"sample"}"#)
                .unwrap();
        let event: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeClientEvent::OutputAudioBufferClearEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value =
            serde_json::from_str(r#"{"type":"input_audio_buffer.commit","event_id":"sample"}"#)
                .unwrap();
        let event: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeClientEvent::InputAudioBufferCommitEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"type":"response.cancel","event_id":"sample","response_id":"sample"}"#,
        )
        .unwrap();
        let event: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeClientEvent::ResponseCancelEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"type":"response.create","event_id":"sample","response":{"audio":{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}},"conversation":"sample","input":[{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}],"instructions":"sample","max_output_tokens":1,"metadata":{"sample":"sample"},"output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}]}}"#).unwrap();
        let event: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeClientEvent::ResponseCreateEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"session":{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"},"type":"session.update","event_id":"sample"}"#).unwrap();
        let event: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(&event, RealtimeClientEvent::SessionUpdateEvent(_)));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"conversation":{"id":"sample","object":"realtime.conversation"},"event_id":"sample","type":"conversation.created"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ConversationCreatedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.created","previous_item_id":"sample"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ConversationItemCreatedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"event_id":"sample","item_id":"sample","type":"conversation.item.deleted"}"#,
        )
        .unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ConversationItemDeletedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","transcript":"sample","type":"conversation.item.input_audio_transcription.completed","usage":{"input_tokens":1,"output_tokens":1,"total_tokens":1,"type":"tokens","input_token_details":{"audio_tokens":1,"text_tokens":1}},"languages":[{"code":"sample"}],"logprobs":[{"token":"sample","bytes":[1],"logprob":0.5}]}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ConversationItemInputAudioTranscriptionCompletedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","item_id":"sample","type":"conversation.item.input_audio_transcription.delta","content_index":1,"delta":"sample","logprobs":[{"token":"sample","bytes":[1],"logprob":0.5}]}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ConversationItemInputAudioTranscriptionDeltaEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"content_index":1,"error":{"code":"sample","message":"sample","param":"sample","type":"sample"},"event_id":"sample","item_id":"sample","type":"conversation.item.input_audio_transcription.failed"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ConversationItemInputAudioTranscriptionFailedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.retrieved"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ConversationItemRetrieved(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"audio_end_ms":1,"content_index":1,"event_id":"sample","item_id":"sample","type":"conversation.item.truncated"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ConversationItemTruncatedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"error":{"message":"sample","type":"sample","code":"sample","event_id":"sample","param":"sample"},"event_id":"sample","type":"error"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(&event, RealtimeServerEvent::RealtimeErrorEvent(_)));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value =
            serde_json::from_str(r#"{"event_id":"sample","type":"input_audio_buffer.cleared"}"#)
                .unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::InputAudioBufferClearedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","item_id":"sample","type":"input_audio_buffer.committed","previous_item_id":"sample"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::InputAudioBufferCommittedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"event":"sample","received_at":1,"type":"input_audio_buffer.dtmf_event_received"}"#,
        )
        .unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::InputAudioBufferDtmfEventReceivedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"audio_start_ms":1,"event_id":"sample","item_id":"sample","type":"input_audio_buffer.speech_started"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::InputAudioBufferSpeechStartedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"audio_end_ms":1,"event_id":"sample","item_id":"sample","type":"input_audio_buffer.speech_stopped"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::InputAudioBufferSpeechStoppedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","rate_limits":[{"limit":1,"name":"requests","remaining":1,"reset_seconds":0.5}],"type":"rate_limits.updated"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::RateLimitsUpdatedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"content_index":1,"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_audio.delta"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseAudioDeltaEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_audio.done"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseAudioDoneEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"content_index":1,"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_audio_transcript.delta"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseAudioTranscriptDeltaEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","transcript":"sample","type":"response.output_audio_transcript.done"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseAudioTranscriptDoneEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"part":{"audio":"sample","text":"sample","transcript":"sample","type":"text"},"response_id":"sample","type":"response.content_part.added"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseContentPartAddedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"part":{"audio":"sample","text":"sample","transcript":"sample","type":"text"},"response_id":"sample","type":"response.content_part.done"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseContentPartDoneEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","response":{"id":"sample","audio":{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}},"conversation_id":"sample","max_output_tokens":1,"metadata":{"sample":"sample"},"object":"realtime.response","output":[{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}],"output_modalities":["text"],"status":"completed","status_details":{"error":{"code":"sample","type":"sample"},"reason":"turn_detected","type":"completed"},"usage":{"input_token_details":{"audio_tokens":1,"cached_tokens":1,"cached_tokens_details":{"audio_tokens":1,"image_tokens":1,"text_tokens":1},"image_tokens":1,"text_tokens":1},"input_tokens":1,"output_token_details":{"audio_tokens":1,"text_tokens":1},"output_tokens":1,"total_tokens":1}},"type":"response.created"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseCreatedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","response":{"id":"sample","audio":{"output":{"format":{"rate":24000,"type":"audio/pcm"},"voice":"sample"}},"conversation_id":"sample","max_output_tokens":1,"metadata":{"sample":"sample"},"object":"realtime.response","output":[{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"}],"output_modalities":["text"],"status":"completed","status_details":{"error":{"code":"sample","type":"sample"},"reason":"turn_detected","type":"completed"},"usage":{"input_token_details":{"audio_tokens":1,"cached_tokens":1,"cached_tokens_details":{"audio_tokens":1,"image_tokens":1,"text_tokens":1},"image_tokens":1,"text_tokens":1},"input_tokens":1,"output_token_details":{"audio_tokens":1,"text_tokens":1},"output_tokens":1,"total_tokens":1}},"type":"response.done"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(&event, RealtimeServerEvent::ResponseDoneEvent(_)));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"call_id":"sample","delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.function_call_arguments.delta"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseFunctionCallArgumentsDeltaEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"arguments":"sample","call_id":"sample","event_id":"sample","item_id":"sample","name":"sample","output_index":1,"response_id":"sample","type":"response.function_call_arguments.done"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseFunctionCallArgumentsDoneEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"output_index":1,"response_id":"sample","type":"response.output_item.added"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseOutputItemAddedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"output_index":1,"response_id":"sample","type":"response.output_item.done"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseOutputItemDoneEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"content_index":1,"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.output_text.delta"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseTextDeltaEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"content_index":1,"event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","text":"sample","type":"response.output_text.done"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseTextDoneEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","session":{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"},"type":"session.created"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::SessionCreatedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","session":{"type":"realtime","audio":{"input":{"format":{"rate":24000,"type":"audio/pcm"},"noise_reduction":{"type":"near_field"},"transcription":{"delay":"minimal","keywords":["sample"],"language":"sample","languages":["sample"],"model":"sample","prompt":"sample"},"turn_detection":{"type":"server_vad","create_response":true,"idle_timeout_ms":1,"interrupt_response":true,"prefix_padding_ms":1,"silence_duration_ms":1,"threshold":0.5}},"output":{"format":{"rate":24000,"type":"audio/pcm"},"speed":0.5,"voice":"sample"}},"include":["item.input_audio_transcription.logprobs"],"instructions":"sample","max_output_tokens":1,"model":"sample","output_modalities":["text"],"parallel_tool_calls":true,"prompt":{"id":"sample","variables":{"sample":"sample"},"version":"sample"},"reasoning":{"effort":"minimal"},"tool_choice":"none","tools":[{"description":"sample","name":"sample","parameters":{"opaque":true},"type":"function"}],"tracing":"auto","truncation":"auto"},"type":"session.updated"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::SessionUpdatedEvent(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"event_id":"sample","response_id":"sample","type":"output_audio_buffer.started"}"#,
        )
        .unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::RealtimeServerEventOutputAudioBufferStarted(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"event_id":"sample","response_id":"sample","type":"output_audio_buffer.stopped"}"#,
        )
        .unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::RealtimeServerEventOutputAudioBufferStopped(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"event_id":"sample","response_id":"sample","type":"output_audio_buffer.cleared"}"#,
        )
        .unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::RealtimeServerEventOutputAudioBufferCleared(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.added","previous_item_id":"sample"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ConversationItemAdded(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","item":{"content":[{"text":"sample","type":"input_text"}],"role":"system","type":"message","id":"sample","object":"realtime.item","status":"completed"},"type":"conversation.item.done","previous_item_id":"sample"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ConversationItemDone(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"audio_end_ms":1,"audio_start_ms":1,"event_id":"sample","item_id":"sample","type":"input_audio_buffer.timeout_triggered"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::InputAudioBufferTimeoutTriggered(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"id":"sample","content_index":1,"end":0.5,"event_id":"sample","item_id":"sample","speaker":"sample","start":0.5,"text":"sample","type":"conversation.item.input_audio_transcription.segment"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ConversationItemInputAudioTranscriptionSegment(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"event_id":"sample","item_id":"sample","type":"mcp_list_tools.in_progress"}"#,
        )
        .unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::McpListToolsInProgress(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"event_id":"sample","item_id":"sample","type":"mcp_list_tools.completed"}"#,
        )
        .unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::McpListToolsCompleted(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value: Value = serde_json::from_str(
            r#"{"event_id":"sample","item_id":"sample","type":"mcp_list_tools.failed"}"#,
        )
        .unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(&event, RealtimeServerEvent::McpListToolsFailed(_)));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"delta":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.mcp_call_arguments.delta","obfuscation":"sample"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseMcpCallArgumentsDelta(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"arguments":"sample","event_id":"sample","item_id":"sample","output_index":1,"response_id":"sample","type":"response.mcp_call_arguments.done"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseMcpCallArgumentsDone(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","item_id":"sample","output_index":1,"type":"response.mcp_call.in_progress"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseMcpCallInProgress(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","item_id":"sample","output_index":1,"type":"response.mcp_call.completed"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseMcpCallCompleted(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
    {
        let value:Value=serde_json::from_str(r#"{"event_id":"sample","item_id":"sample","output_index":1,"type":"response.mcp_call.failed"}"#).unwrap();
        let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            &event,
            RealtimeServerEvent::ResponseMcpCallFailed(_)
        ));
        assert_eq!(serde_json::to_value(event).unwrap(), value);
    }
}
#[test]
fn all_source_literal_values_are_closed() {
    literal::<RealtimeSessionCreateRequestType>(&[json!("realtime")]);
    literal::<RealtimeSessionCreateRequestIncludeItem>(&[json!(
        "item.input_audio_transcription.logprobs"
    )]);
    literal::<RealtimeSessionCreateRequestMaxOutputTokensVariant2>(&[json!("inf")]);
    literal::<RealtimeSessionCreateRequestOutputModalitiesItem>(&[json!("text"), json!("audio")]);
    literal::<RealtimeSessionCreateResponseObject>(&[json!("realtime.session")]);
    literal::<RealtimeSessionCreateResponseType>(&[json!("realtime")]);
    literal::<RealtimeSessionCreateResponseIncludeItem>(&[json!(
        "item.input_audio_transcription.logprobs"
    )]);
    literal::<RealtimeSessionCreateResponseMaxOutputTokensVariant2>(&[json!("inf")]);
    literal::<RealtimeSessionCreateResponseOutputModalitiesItem>(&[json!("text"), json!("audio")]);
    literal::<RealtimeTranscriptionSessionCreateResponseType>(&[json!("transcription")]);
    literal::<RealtimeTranscriptionSessionCreateResponseIncludeItem>(&[json!(
        "item.input_audio_transcription.logprobs"
    )]);
    literal::<ConversationItemCreateEventType>(&[json!("conversation.item.create")]);
    literal::<ConversationItemDeleteEventType>(&[json!("conversation.item.delete")]);
    literal::<ConversationItemRetrieveEventType>(&[json!("conversation.item.retrieve")]);
    literal::<ConversationItemTruncateEventType>(&[json!("conversation.item.truncate")]);
    literal::<InputAudioBufferAppendEventType>(&[json!("input_audio_buffer.append")]);
    literal::<InputAudioBufferClearEventType>(&[json!("input_audio_buffer.clear")]);
    literal::<OutputAudioBufferClearEventType>(&[json!("output_audio_buffer.clear")]);
    literal::<InputAudioBufferCommitEventType>(&[json!("input_audio_buffer.commit")]);
    literal::<ResponseCancelEventType>(&[json!("response.cancel")]);
    literal::<ResponseCreateEventType>(&[json!("response.create")]);
    literal::<SessionUpdateEventType>(&[json!("session.update")]);
    literal::<ConversationCreatedEventType>(&[json!("conversation.created")]);
    literal::<ConversationItemCreatedEventType>(&[json!("conversation.item.created")]);
    literal::<ConversationItemDeletedEventType>(&[json!("conversation.item.deleted")]);
    literal::<ConversationItemInputAudioTranscriptionCompletedEventType>(&[json!(
        "conversation.item.input_audio_transcription.completed"
    )]);
    literal::<ConversationItemInputAudioTranscriptionDeltaEventType>(&[json!(
        "conversation.item.input_audio_transcription.delta"
    )]);
    literal::<ConversationItemInputAudioTranscriptionFailedEventType>(&[json!(
        "conversation.item.input_audio_transcription.failed"
    )]);
    literal::<ConversationItemRetrievedType>(&[json!("conversation.item.retrieved")]);
    literal::<ConversationItemTruncatedEventType>(&[json!("conversation.item.truncated")]);
    literal::<RealtimeErrorEventType>(&[json!("error")]);
    literal::<InputAudioBufferClearedEventType>(&[json!("input_audio_buffer.cleared")]);
    literal::<InputAudioBufferCommittedEventType>(&[json!("input_audio_buffer.committed")]);
    literal::<InputAudioBufferDtmfEventReceivedEventType>(&[json!(
        "input_audio_buffer.dtmf_event_received"
    )]);
    literal::<InputAudioBufferSpeechStartedEventType>(&[json!(
        "input_audio_buffer.speech_started"
    )]);
    literal::<InputAudioBufferSpeechStoppedEventType>(&[json!(
        "input_audio_buffer.speech_stopped"
    )]);
    literal::<RateLimitsUpdatedEventType>(&[json!("rate_limits.updated")]);
    literal::<ResponseAudioDeltaEventType>(&[json!("response.output_audio.delta")]);
    literal::<ResponseAudioDoneEventType>(&[json!("response.output_audio.done")]);
    literal::<ResponseAudioTranscriptDeltaEventType>(&[json!(
        "response.output_audio_transcript.delta"
    )]);
    literal::<ResponseAudioTranscriptDoneEventType>(&[json!(
        "response.output_audio_transcript.done"
    )]);
    literal::<ResponseContentPartAddedEventType>(&[json!("response.content_part.added")]);
    literal::<ResponseContentPartDoneEventType>(&[json!("response.content_part.done")]);
    literal::<ResponseCreatedEventType>(&[json!("response.created")]);
    literal::<ResponseDoneEventType>(&[json!("response.done")]);
    literal::<ResponseFunctionCallArgumentsDeltaEventType>(&[json!(
        "response.function_call_arguments.delta"
    )]);
    literal::<ResponseFunctionCallArgumentsDoneEventType>(&[json!(
        "response.function_call_arguments.done"
    )]);
    literal::<ResponseOutputItemAddedEventType>(&[json!("response.output_item.added")]);
    literal::<ResponseOutputItemDoneEventType>(&[json!("response.output_item.done")]);
    literal::<ResponseTextDeltaEventType>(&[json!("response.output_text.delta")]);
    literal::<ResponseTextDoneEventType>(&[json!("response.output_text.done")]);
    literal::<SessionCreatedEventType>(&[json!("session.created")]);
    literal::<SessionUpdatedEventType>(&[json!("session.updated")]);
    literal::<RealtimeServerEventOutputAudioBufferStartedType>(&[json!(
        "output_audio_buffer.started"
    )]);
    literal::<RealtimeServerEventOutputAudioBufferStoppedType>(&[json!(
        "output_audio_buffer.stopped"
    )]);
    literal::<RealtimeServerEventOutputAudioBufferClearedType>(&[json!(
        "output_audio_buffer.cleared"
    )]);
    literal::<ConversationItemAddedType>(&[json!("conversation.item.added")]);
    literal::<ConversationItemDoneType>(&[json!("conversation.item.done")]);
    literal::<InputAudioBufferTimeoutTriggeredType>(&[json!(
        "input_audio_buffer.timeout_triggered"
    )]);
    literal::<ConversationItemInputAudioTranscriptionSegmentType>(&[json!(
        "conversation.item.input_audio_transcription.segment"
    )]);
    literal::<McpListToolsInProgressType>(&[json!("mcp_list_tools.in_progress")]);
    literal::<McpListToolsCompletedType>(&[json!("mcp_list_tools.completed")]);
    literal::<McpListToolsFailedType>(&[json!("mcp_list_tools.failed")]);
    literal::<ResponseMcpCallArgumentsDeltaType>(&[json!("response.mcp_call_arguments.delta")]);
    literal::<ResponseMcpCallArgumentsDoneType>(&[json!("response.mcp_call_arguments.done")]);
    literal::<ResponseMcpCallInProgressType>(&[json!("response.mcp_call.in_progress")]);
    literal::<ResponseMcpCallCompletedType>(&[json!("response.mcp_call.completed")]);
    literal::<ResponseMcpCallFailedType>(&[json!("response.mcp_call.failed")]);
    literal::<RealtimeTracingConfigVariant1>(&[json!("auto")]);
    literal::<RealtimeTruncationVariant1>(&[json!("auto"), json!("disabled")]);
    literal::<RealtimeSessionCreateResponseTracingVariant1>(&[json!("auto")]);
    literal::<RealtimeResponseCreateParamsMaxOutputTokensVariant2>(&[json!("inf")]);
    literal::<RealtimeResponseCreateParamsOutputModalitiesItem>(&[json!("text"), json!("audio")]);
    literal::<ConversationCreatedEventConversationObject>(&[json!("realtime.conversation")]);
    literal::<RateLimitsUpdatedEventRateLimitName>(&[json!("requests"), json!("tokens")]);
    literal::<ResponseContentPartAddedEventPartType>(&[json!("text"), json!("audio")]);
    literal::<ResponseContentPartDoneEventPartType>(&[json!("text"), json!("audio")]);
    literal::<RealtimeResponseMaxOutputTokensVariant2>(&[json!("inf")]);
    literal::<RealtimeResponseObject>(&[json!("realtime.response")]);
    literal::<RealtimeResponseOutputModalitiesItem>(&[json!("text"), json!("audio")]);
    literal::<RealtimeResponseStatusValue>(&[
        json!("completed"),
        json!("cancelled"),
        json!("failed"),
        json!("incomplete"),
        json!("in_progress"),
    ]);
    literal::<RealtimeReasoningEffort>(&[
        json!("minimal"),
        json!("low"),
        json!("medium"),
        json!("high"),
        json!("xhigh"),
    ]);
    literal::<ToolChoiceOptions>(&[json!("none"), json!("auto"), json!("required")]);
    literal::<ToolChoiceFunctionType>(&[json!("function")]);
    literal::<ToolChoiceMcpType>(&[json!("mcp")]);
    literal::<RealtimeTruncationRetentionRatioType>(&[json!("retention_ratio")]);
    literal::<RealtimeFunctionToolType>(&[json!("function")]);
    literal::<RealtimeSessionCreateResponseToolMcpToolType>(&[json!("mcp")]);
    literal::<RealtimeSessionCreateResponseToolMcpToolAllowedCallersItem>(&[
        json!("direct"),
        json!("programmatic"),
    ]);
    literal::<RealtimeSessionCreateResponseToolMcpToolConnectorId>(&[
        json!("connector_dropbox"),
        json!("connector_gmail"),
        json!("connector_googlecalendar"),
        json!("connector_googledrive"),
        json!("connector_microsoftteams"),
        json!("connector_outlookcalendar"),
        json!("connector_outlookemail"),
        json!("connector_sharepoint"),
    ]);
    literal::<RealtimeConversationItemSystemMessageRole>(&[json!("system")]);
    literal::<RealtimeConversationItemSystemMessageType>(&[json!("message")]);
    literal::<RealtimeConversationItemSystemMessageObject>(&[json!("realtime.item")]);
    literal::<RealtimeConversationItemSystemMessageStatus>(&[
        json!("completed"),
        json!("incomplete"),
        json!("in_progress"),
    ]);
    literal::<RealtimeConversationItemUserMessageRole>(&[json!("user")]);
    literal::<RealtimeConversationItemUserMessageType>(&[json!("message")]);
    literal::<RealtimeConversationItemUserMessageObject>(&[json!("realtime.item")]);
    literal::<RealtimeConversationItemUserMessageStatus>(&[
        json!("completed"),
        json!("incomplete"),
        json!("in_progress"),
    ]);
    literal::<RealtimeConversationItemAssistantMessageRole>(&[json!("assistant")]);
    literal::<RealtimeConversationItemAssistantMessageType>(&[json!("message")]);
    literal::<RealtimeConversationItemAssistantMessageObject>(&[json!("realtime.item")]);
    literal::<RealtimeConversationItemAssistantMessageStatus>(&[
        json!("completed"),
        json!("incomplete"),
        json!("in_progress"),
    ]);
    literal::<RealtimeConversationItemFunctionCallType>(&[json!("function_call")]);
    literal::<RealtimeConversationItemFunctionCallObject>(&[json!("realtime.item")]);
    literal::<RealtimeConversationItemFunctionCallStatus>(&[
        json!("completed"),
        json!("incomplete"),
        json!("in_progress"),
    ]);
    literal::<RealtimeConversationItemFunctionCallOutputType>(&[json!("function_call_output")]);
    literal::<RealtimeConversationItemFunctionCallOutputObject>(&[json!("realtime.item")]);
    literal::<RealtimeConversationItemFunctionCallOutputStatus>(&[
        json!("completed"),
        json!("incomplete"),
        json!("in_progress"),
    ]);
    literal::<RealtimeMcpApprovalResponseType>(&[json!("mcp_approval_response")]);
    literal::<RealtimeMcpListToolsType>(&[json!("mcp_list_tools")]);
    literal::<RealtimeMcpToolCallType>(&[json!("mcp_call")]);
    literal::<RealtimeMcpApprovalRequestType>(&[json!("mcp_approval_request")]);
    literal::<RealtimeTranscriptionSessionCreateRequestType>(&[json!("transcription")]);
    literal::<RealtimeTranscriptionSessionCreateRequestIncludeItem>(&[json!(
        "item.input_audio_transcription.logprobs"
    )]);
    literal::<
        ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageTokensType,
    >(&[json!("tokens")]);
    literal::<
        ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageDurationType,
    >(&[json!("duration")]);
    literal::<RealtimeResponseStatusReason>(&[
        json!("turn_detected"),
        json!("client_cancelled"),
        json!("max_output_tokens"),
        json!("content_filter"),
    ]);
    literal::<RealtimeResponseStatusType>(&[
        json!("completed"),
        json!("cancelled"),
        json!("incomplete"),
        json!("failed"),
    ]);
    literal::<AudioTranscriptionDelay>(&[
        json!("minimal"),
        json!("low"),
        json!("medium"),
        json!("high"),
        json!("xhigh"),
    ]);
    literal::<ResponseInputTextType>(&[json!("input_text")]);
    literal::<ResponseInputImageType>(&[json!("input_image")]);
    literal::<ResponseInputFileType>(&[json!("input_file")]);
    literal::<ResponseInputFileDetail>(&[json!("auto"), json!("low"), json!("high")]);
    literal::<RealtimeToolsConfigUnionMcpType>(&[json!("mcp")]);
    literal::<RealtimeToolsConfigUnionMcpAllowedCallersItem>(&[
        json!("direct"),
        json!("programmatic"),
    ]);
    literal::<RealtimeToolsConfigUnionMcpConnectorId>(&[
        json!("connector_dropbox"),
        json!("connector_gmail"),
        json!("connector_googlecalendar"),
        json!("connector_googledrive"),
        json!("connector_microsoftteams"),
        json!("connector_outlookcalendar"),
        json!("connector_outlookemail"),
        json!("connector_sharepoint"),
    ]);
    literal::<RealtimeSessionCreateResponseToolMcpToolRequireApprovalVariant2>(&[
        json!("always"),
        json!("never"),
    ]);
    literal::<RealtimeConversationItemSystemMessageContentType>(&[json!("input_text")]);
    literal::<RealtimeConversationItemUserMessageContentDetail>(&[
        json!("auto"),
        json!("low"),
        json!("high"),
    ]);
    literal::<RealtimeConversationItemUserMessageContentType>(&[
        json!("input_text"),
        json!("input_audio"),
        json!("input_image"),
    ]);
    literal::<RealtimeConversationItemAssistantMessageContentType>(&[
        json!("output_text"),
        json!("output_audio"),
    ]);
    literal::<RealtimeResponseCreateMcpToolType>(&[json!("mcp")]);
    literal::<RealtimeResponseCreateMcpToolAllowedCallersItem>(&[
        json!("direct"),
        json!("programmatic"),
    ]);
    literal::<RealtimeResponseCreateMcpToolConnectorId>(&[
        json!("connector_dropbox"),
        json!("connector_gmail"),
        json!("connector_googlecalendar"),
        json!("connector_googledrive"),
        json!("connector_microsoftteams"),
        json!("connector_outlookcalendar"),
        json!("connector_outlookemail"),
        json!("connector_sharepoint"),
    ]);
    literal::<RealtimeAudioFormatsAudioPCMRate>(&[json!(24000)]);
    literal::<RealtimeAudioFormatsAudioPCMType>(&[json!("audio/pcm")]);
    literal::<RealtimeAudioFormatsAudioPCMUType>(&[json!("audio/pcmu")]);
    literal::<RealtimeAudioFormatsAudioPCMAType>(&[json!("audio/pcma")]);
    literal::<NoiseReductionType>(&[json!("near_field"), json!("far_field")]);
    literal::<RealtimeAudioInputTurnDetectionServerVadType>(&[json!("server_vad")]);
    literal::<RealtimeAudioInputTurnDetectionSemanticVadType>(&[json!("semantic_vad")]);
    literal::<RealtimeAudioInputTurnDetectionSemanticVadEagerness>(&[
        json!("low"),
        json!("medium"),
        json!("high"),
        json!("auto"),
    ]);
    literal::<ResponseInputTextPromptCacheBreakpointMode>(&[json!("explicit")]);
    literal::<ImageDetail>(&[
        json!("low"),
        json!("high"),
        json!("auto"),
        json!("original"),
    ]);
    literal::<ResponseInputImagePromptCacheBreakpointMode>(&[json!("explicit")]);
    literal::<ResponseInputFilePromptCacheBreakpointMode>(&[json!("explicit")]);
    literal::<RealtimeToolsConfigUnionMcpRequireApprovalVariant2>(&[
        json!("always"),
        json!("never"),
    ]);
    literal::<RealtimeSessionCreateResponseAudioInputTurnDetectionServerVadType>(&[json!(
        "server_vad"
    )]);
    literal::<RealtimeSessionCreateResponseAudioInputTurnDetectionSemanticVadType>(&[json!(
        "semantic_vad"
    )]);
    literal::<RealtimeSessionCreateResponseAudioInputTurnDetectionSemanticVadEagerness>(&[
        json!("low"),
        json!("medium"),
        json!("high"),
        json!("auto"),
    ]);
    literal::<RealtimeMcpProtocolErrorType>(&[json!("protocol_error")]);
    literal::<RealtimeMcpToolExecutionErrorType>(&[json!("tool_execution_error")]);
    literal::<RealtimeMcphttpErrorType>(&[json!("http_error")]);
    literal::<RealtimeResponseCreateMcpToolRequireApprovalVariant2>(&[
        json!("always"),
        json!("never"),
    ]);
    literal::<RealtimeTranscriptionSessionAudioInputTurnDetectionServerVadType>(&[json!(
        "server_vad"
    )]);
    literal::<RealtimeTranscriptionSessionAudioInputTurnDetectionSemanticVadType>(&[json!(
        "semantic_vad"
    )]);
    literal::<RealtimeTranscriptionSessionAudioInputTurnDetectionSemanticVadEagerness>(&[
        json!("low"),
        json!("medium"),
        json!("high"),
        json!("auto"),
    ]);
    literal::<ConversationItemWithReferenceObject>(&[json!("realtime.item")]);
    literal::<ConversationItemWithReferenceRole>(&[
        json!("user"),
        json!("assistant"),
        json!("system"),
    ]);
    literal::<ConversationItemWithReferenceStatus>(&[
        json!("completed"),
        json!("incomplete"),
        json!("in_progress"),
    ]);
    literal::<ConversationItemWithReferenceType>(&[
        json!("message"),
        json!("function_call"),
        json!("function_call_output"),
    ]);
    literal::<ConversationItemWithReferenceContentType>(&[
        json!("input_audio"),
        json!("input_text"),
        json!("item_reference"),
        json!("text"),
    ]);
}

#[test]
fn session_update_selects_realtime_or_transcription_without_extra_event_tags() {
    for session in [
        json!({"type":"realtime","model":"future-model","audio":{"input":{"format":{"type":"audio/pcm","rate":24000},"noise_reduction":{"type":"near_field"},"turn_detection":null},"output":{"voice":"marin"}},"output_modalities":["audio"],"tools":[{"type":"mcp","server_label":"docs","server_url":"https://example.test","allowed_tools":["read"],"require_approval":"never"}],"tool_choice":"auto"}),
        json!({"type":"transcription","audio":{"input":{"format":{"type":"audio/pcmu"},"transcription":{"model":"gpt-4o-transcribe","language":"en"},"turn_detection":{"type":"server_vad","threshold":0.5}}},"include":["item.input_audio_transcription.logprobs"]}),
    ] {
        let value = json!({"type":"session.update","session":session});
        round_trip::<RealtimeClientEvent>(value, true);
    }
    assert!(
        serde_json::from_value::<RealtimeClientEvent>(
            json!({"type":"transcription_session.update","session":{"type":"transcription"}})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<RealtimeAudioFormats>(json!({"type":"audio/pcm","rate":48000}))
            .is_err()
    );
    let rate = RealtimeAudioFormatsAudioPCMRate::try_from(24000).unwrap();
    assert_eq!(serde_json::to_value(rate).unwrap(), json!(24000));
    assert!(RealtimeAudioFormatsAudioPCMRate::try_from(16000).is_err());
    let session = RealtimeSessionCreateRequest::builder(RealtimeSessionCreateRequestType::Realtime)
        .model(Some("future-model".to_owned()))
        .build();
    let event = SessionUpdateEvent::builder(
        SessionUpdateEventSession::RealtimeSessionCreateRequest(session),
        SessionUpdateEventType::SessionUpdate,
    )
    .build();
    assert_eq!(
        serde_json::to_value(event).unwrap(),
        json!({"type":"session.update","session":{"type":"realtime","model":"future-model"}})
    );
}

#[test]
fn explicit_event_tags_do_not_get_swallowed_by_unrelated_optional_objects() {
    let value = json!({"type":"response.output_audio.delta","event_id":"e","response_id":"r","item_id":"i","output_index":0,"content_index":0,"delta":"AQI=","session":{"type":"realtime"},"error":{"type":"extension"}});
    let event: RealtimeServerEvent = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(
        event,
        RealtimeServerEvent::ResponseAudioDeltaEvent(_)
    ));
    assert_eq!(serde_json::to_value(event).unwrap(), value);
    for value in [
        json!({"type":"conversation.item.delete"}),
        json!({"type":"session.update"}),
        json!({"type":"response.output_audio.delta","delta":"a"}),
    ] {
        assert!(serde_json::from_value::<RealtimeClientEvent>(value.clone()).is_err());
        assert!(serde_json::from_value::<RealtimeServerEvent>(value).is_err());
    }
    assert!(serde_json::from_str::<RealtimeServerEvent>("[DONE]").is_err());
    let event: RealtimeClientEvent = serde_json::from_value(
        json!({"type":"input_audio_buffer.commit","future_extension":{"x":1}}),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(event).unwrap(),
        json!({"type":"input_audio_buffer.commit","future_extension":{"x":1}})
    );
}

#[test]
fn webrtc_form_sdp_response_and_websocket_handshake_keep_transport_boundaries() {
    use gproxy_protocol::{
        WireRequest, WireResponse,
        connection::{HttpBody, MultipartPart},
    };
    use http::{HeaderMap, Method, StatusCode};
    let mut part_headers = HeaderMap::new();
    part_headers.insert(
        "content-disposition",
        "form-data; name=\"sdp\"".parse().unwrap(),
    );
    part_headers.insert("content-type", "application/sdp".parse().unwrap());
    let session =
        RealtimeSessionCreateRequest::builder(RealtimeSessionCreateRequestType::Realtime).build();
    assert_eq!(
        serde_json::to_value(&session).unwrap(),
        json!({"type":"realtime"})
    );
    let form = CreateRealtimeCallMultipartForm::builder(MultipartPart {
        headers: part_headers.clone(),
        body: HttpBody::Bytes(bytes::Bytes::from_static(b"v=0\r\n")),
    })
    .session(session)
    .build();
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "multipart/form-data; boundary=wire".parse().unwrap(),
    );
    let request: CreateRealtimeCallRequest = WireRequest {
        method: Method::POST,
        path: "/v1/realtime/calls".into(),
        query: None,
        headers: headers.clone(),
        body: form,
    };
    assert_eq!(request.method, Method::POST);
    assert_eq!(request.path, "/v1/realtime/calls");
    assert!(request.query.is_none());
    assert_eq!(request.headers, headers);
    assert_eq!(request.body.sdp.headers, part_headers);
    let mut response_headers = HeaderMap::new();
    response_headers.insert("location", "/v1/realtime/calls/rtc_123".parse().unwrap());
    response_headers.insert("content-type", "application/sdp".parse().unwrap());
    let response: CreateRealtimeCallResponse = WireResponse {
        status: StatusCode::CREATED,
        headers: response_headers.clone(),
        body: HttpBody::Bytes(bytes::Bytes::from_static(b"v=0\r\na=answer\r\n")),
    };
    assert_eq!(response.status, StatusCode::CREATED);
    assert_eq!(response.headers, response_headers);
    assert!(
        matches!(response.body,HttpBody::Bytes(ref bytes) if bytes.as_ref()==b"v=0\r\na=answer\r\n")
    );
    let handshake: HandshakeRequest = WireRequest {
        method: Method::GET,
        path: "/v1/realtime".into(),
        query: Some("model=gpt-realtime".into()),
        headers: HeaderMap::new(),
        body: (),
    };
    assert_eq!(handshake.query.as_deref(), Some("model=gpt-realtime"));
    let upgraded: HandshakeResponse = WireResponse {
        status: StatusCode::SWITCHING_PROTOCOLS,
        headers: HeaderMap::new(),
        body: (),
    };
    assert_eq!(upgraded.status.as_u16(), 101);
}

#[test]
fn legacy_reference_requires_its_documented_id() {
    round_trip::<ConversationItemWithReference>(json!({"type":"item_reference","id":"i"}), true);
    assert!(
        serde_json::from_value::<ConversationItemWithReference>(json!({"type":"item_reference"}))
            .is_err()
    );
}
