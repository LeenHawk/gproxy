use super::*;
use gproxy_protocol::{
    adapt::generate::{
        chat_claude::ClaudeViaChat,
        chat_gemini::*,
        chat_responses::*,
        claude_gemini::*,
        claude_responses::*,
        gemini_responses::*,
        stream::{bridge::StreamBridge, event::NativeEvent},
    },
    transform::{
        generate::{self as p, stream as native},
        identity::IdentityFlow,
    },
    wire::{claude::generate_content as c, openai::responses as r},
};
pub(super) fn request(d: Dialect) -> Value {
    let schema = json!({"type":"object","properties":{"x":{"type":"number"}}});
    match d {
        Dialect::OpenAiChat => {
            json!({"model":"client","stream":true,"stream_options":{"include_usage":true},"max_completion_tokens":64,"messages":[{"role":"user","content":"hi"}],"tools":[{"type":"function","function":{"name":"f","parameters":schema}}]})
        }
        Dialect::Claude => {
            json!({"model":"client","stream":true,"max_tokens":64,"messages":[{"role":"user","content":"hi"}],"tools":[{"name":"f","input_schema":schema}]})
        }
        Dialect::Gemini => {
            json!({"contents":[{"role":"user","parts":[{"text":"hi"}]}],"generationConfig":{"maxOutputTokens":64},"tools":[{"functionDeclarations":[{"name":"f","description":"lookup","parameters":{"type":"OBJECT","properties":{"x":{"type":"NUMBER"}}}}]}]})
        }
        Dialect::OpenAi => {
            json!({"model":"client","stream":true,"max_output_tokens":64,"input":"hi","parallel_tool_calls":true,"tool_choice":"auto","tools":[{"type":"function","name":"f","parameters":schema,"strict":false}]})
        }
        _ => unreachable!(),
    }
}
fn usage() -> Value {
    json!({"input_tokens":3,"output_tokens":2,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}})
}
fn initial() -> c::Usage {
    let mut value = usage();
    value["output_tokens"] = json!(0);
    serde_json::from_value(value).unwrap()
}
pub(super) fn native_response(d: Dialect) -> Value {
    match d {
        Dialect::Claude => {
            json!({"id":"message:source","type":"message","role":"assistant","model":"selected","content":[{"type":"text","text":"answer"},{"type":"tool_use","id":"tool:source","name":"f","input":{"x":1}}],"stop_reason":"tool_use","stop_sequence":null,"usage":usage()})
        }
        Dialect::OpenAiChat => {
            json!({"id":"chat:source","object":"chat.completion","created":7,"model":"selected","choices":[{"index":0,"message":{"role":"assistant","content":"answer","refusal":null,"tool_calls":[{"id":"tool:source","type":"function","function":{"name":"f","arguments":"{\"x\":1}"}}]},"finish_reason":"tool_calls","logprobs":null}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5,"prompt_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"completion_tokens_details":{"reasoning_tokens":0}}})
        }
        Dialect::Gemini => {
            json!({"responseId":"gemini:source","modelVersion":"selected","candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"answer"},{"functionCall":{"id":"tool:source","name":"f","args":{"x":1}}}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2,"totalTokenCount":5,"cachedContentTokenCount":0,"thoughtsTokenCount":0,"toolUsePromptTokenCount":0}})
        }
        Dialect::OpenAi => {
            json!({"id":"resp:source","object":"response","created_at":7,"model":"selected","status":"completed","error":null,"incomplete_details":null,"instructions":null,"metadata":null,"parallel_tool_calls":false,"temperature":null,"top_p":null,"tool_choice":"auto","tools":[],"output":[{"id":"msg:source","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"answer","annotations":[],"logprobs":[]}]},{"id":"fc:source","type":"function_call","call_id":"tool:source","name":"f","arguments":"{\"x\":1}","status":"completed"}],"usage":{"input_tokens":3,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens":2,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":5}})
        }
        _ => unreachable!(),
    }
}
fn feed_events<E: NativeEvent>(feed: &Feed, events: Vec<E>) {
    for event in events {
        let name = event
            .event_name()
            .map(|v| format!("event: {v}\n"))
            .unwrap_or_default();
        feed.push(format!(
            "{name}data: {}\n\n",
            serde_json::to_string(&event).unwrap()
        ));
    }
    if E::DONE {
        feed.push("data: [DONE]\n\n");
    }
}
pub(super) fn source(feed: &Feed, d: Dialect) {
    let body = native_response(d);
    match d {
        Dialect::Claude => feed_events(
            feed,
            native::claude::synthesize_claude_stream(
                serde_json::from_value(body).unwrap(),
                Default::default(),
            )
            .unwrap()
            .value,
        ),
        Dialect::OpenAiChat => feed_events(
            feed,
            native::chat::synthesize_chat_stream(
                serde_json::from_value(body).unwrap(),
                Default::default(),
            )
            .unwrap()
            .value,
        ),
        Dialect::Gemini => feed_events(
            feed,
            native::gemini::synthesize_gemini_stream(
                serde_json::from_value(body).unwrap(),
                Default::default(),
            )
            .unwrap()
            .value,
        ),
        Dialect::OpenAi => feed_events(
            feed,
            native::responses::synthesize_responses_stream(
                serde_json::from_value(body).unwrap(),
                &mut IdentityFlow::new(IdNamespace([90; 16])),
                Default::default(),
            )
            .unwrap()
            .value,
        ),
        _ => unreachable!(),
    }
}
pub(super) fn access(store: &Store, native: Dialect) -> GenerationStateAccess<'_, Store> {
    let mut value = state(store);
    value.target = IdentityTarget::new("selected", native)
        .unwrap()
        .with_origin("origin")
        .unwrap();
    value
}
pub(super) fn target(client: Dialect, native: Dialect) -> StreamTarget {
    let mut value = selected();
    value.identities =
        GenerationIdentity::new(IdNamespace([61; 16]), IdNamespace([62; 16]), client, native)
            .unwrap();
    value.identities.response_policy = value
        .identities
        .response_policy
        .with_syntax(IdSyntax::AsciiIdentifier);
    value
}
fn run<B: StreamBridge>(mut call: StreamInvocation<B>, store: Arc<Store>) {
    let feed = Feed::default();
    source(&feed, B::NativeEvent::DIALECT);
    let host = Host::stream(store.clone(), feed.clone());
    let state = access(&store, B::NativeEvent::DIALECT);
    ready(call.start(&host, &(), &state)).unwrap();
    let mut bytes = Vec::new();
    let mut live = false;
    for _ in 0..64 {
        let event = ready(call.next(&state)).unwrap().unwrap();
        bytes.extend_from_slice(&event.bytes);
        if String::from_utf8_lossy(&bytes).contains("answer") {
            live = true;
            break;
        }
    }
    assert!(
        live,
        "{:?} -> {:?} did not emit before EOF",
        B::NativeEvent::DIALECT,
        B::ClientEvent::DIALECT
    );
    assert!(call.client_result().is_none());
    feed.close();
    while let Some(event) = ready(call.next(&state)).unwrap() {
        bytes.extend_from_slice(&event.bytes);
    }
    let client = serde_json::to_value(call.client_result().unwrap()).unwrap();
    assert!(client.to_string().contains("answer"));
    let id = match B::ClientEvent::DIALECT {
        Dialect::OpenAiChat => client["choices"][0]["message"]["tool_calls"][0]["id"]
            .as_str()
            .unwrap(),
        Dialect::Claude => client["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["type"] == "tool_use")
            .unwrap()["id"]
            .as_str()
            .unwrap(),
        Dialect::Gemini => client["candidates"][0]["content"]["parts"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|v| v.get("functionCall"))
            .unwrap()["id"]
            .as_str()
            .unwrap(),
        Dialect::OpenAi => client["output"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["type"] == "function_call")
            .unwrap()["call_id"]
            .as_str()
            .unwrap(),
        _ => unreachable!(),
    };
    // The policy rejects the upstream ID, so the client gets its reversible
    // alias, which names the ID itself: nothing is recorded for it.
    let prefix = if B::ClientEvent::DIALECT == Dialect::Claude {
        "toolu_"
    } else {
        "call_"
    };
    assert_eq!(id, format!("{prefix}gpe_tool_3asource"));
    assert!(
        ready(state.read(IdentityRole::ToolCall, id))
            .unwrap()
            .is_none()
    );
    if B::ClientEvent::DIALECT == Dialect::OpenAi {
        let item_id = client["output"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["type"] == "function_call")
            .unwrap()["id"]
            .as_str()
            .unwrap();
        // An unsigned output item is never read back, so it has no record.
        assert!(
            ready(state.read(
                IdentityRole::OutputItem(
                    gproxy_protocol::transform::identity::OutputItemKind::FunctionCall,
                ),
                item_id,
            ))
            .unwrap()
            .is_none()
        );
    }
    // No identity is recorded at all: no Response, Message, call or stream
    // alias record.
    assert!(
        !store
            .entries
            .lock()
            .unwrap()
            .keys()
            .any(|key| key.starts_with("generate:") || key.starts_with("stream:"))
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
fn auto() -> r::ToolChoice {
    r::ToolChoice::Mode(r::ToolChoiceMode::Auto)
}
pub(super) fn chat_response_context() -> p::chat_responses::stream::ChatToResponsesContext {
    p::chat_responses::ResponsesResponseContext {
        request: serde_json::from_value(request(Dialect::OpenAi)).unwrap(),
        effective_parallel_tool_calls: true,
        effective_tool_choice: auto(),
        usage: Default::default(),
        effective_prompt_cache_options: None,
    }
    .into()
}
pub(super) fn claude_response_context() -> p::claude_responses::stream::ClaudeToResponsesContext {
    p::claude_responses::ClaudeResponseContext {
        request: serde_json::from_value(request(Dialect::OpenAi)).unwrap(),
        created_at: 7,
        effective_parallel_tool_calls: true,
        effective_tool_choice: auto(),
        usage: Default::default(),
        effective_prompt_cache_options: None,
    }
    .into()
}
pub(super) fn gemini_response_context() -> p::gemini_responses::stream::GeminiToResponsesContext {
    p::gemini_responses::stream::GeminiToResponsesContext {
        response: p::gemini_responses::GeminiResponseContext {
            request: serde_json::from_value(request(Dialect::OpenAi)).unwrap(),
            created_at: 7,
            effective_parallel_tool_calls: true,
            effective_tool_choice: auto(),
            usage: p::gemini_responses::GeminiUsageFacts {
                cache_write_tokens: Some(0),
                ..Default::default()
            },
            effective_prompt_cache_options: None,
        },
        actual_model: Some("selected".into()),
        final_thinking_tokens: Some(0),
    }
}
macro_rules! case {
    ($name:ident, $adapter:ty, $client:expr, $native:expr $(, $context:expr)?) => {
        #[test] fn $name() {
            let store = Arc::new(Store::default()); let state = access(&store, $native);
            let call = ready(<$adapter>::prepare_stream(serde_json::from_value(request($client)).unwrap(), target($client,$native), $($context,)? settings(), &state)).unwrap();
            run(call, store);
        }
    };
}
case!(
    chat_via_claude,
    ChatViaClaude,
    Dialect::OpenAiChat,
    Dialect::Claude,
    ClaudeToChatContext { created: 7 }
);
case!(
    claude_via_chat,
    ClaudeViaChat,
    Dialect::Claude,
    Dialect::OpenAiChat,
    p::claude_chat::stream::ChatToClaudeContext {
        start_usage: Some(initial())
    }
);
case!(
    chat_via_responses,
    ChatViaResponses,
    Dialect::OpenAiChat,
    Dialect::OpenAi
);
case!(
    responses_via_chat,
    ResponsesViaChat,
    Dialect::OpenAi,
    Dialect::OpenAiChat,
    chat_response_context()
);
case!(
    gemini_via_chat,
    GeminiViaChat,
    Dialect::Gemini,
    Dialect::OpenAiChat
);
case!(
    chat_via_gemini,
    ChatViaGemini,
    Dialect::OpenAiChat,
    Dialect::Gemini,
    ChatViaGeminiStreamFacts {
        function_names: Default::default(),
        response: p::gemini_chat::stream::GeminiToChatContext {
            created: 7,
            model: Some("selected".into())
        }
    }
);
case!(
    claude_via_gemini,
    ClaudeViaGemini,
    Dialect::Claude,
    Dialect::Gemini,
    ClaudeViaGeminiStreamFacts {
        request: Default::default(),
        response: p::claude_gemini::stream::GeminiToClaudeContext {
            usage: Some(initial()),
            ..Default::default()
        }
    }
);
case!(
    gemini_via_claude,
    GeminiViaClaude,
    Dialect::Gemini,
    Dialect::Claude,
    GeminiViaClaudeStreamFacts {
        max_tokens: Some(64),
        response: p::claude_gemini::stream::ClaudeToGeminiContext {
            usage: p::claude_gemini::ClaudeGeminiUsageFacts {
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                thinking_tokens: Some(0)
            }
        }
    }
);
case!(
    claude_via_responses,
    ClaudeViaResponses,
    Dialect::Claude,
    Dialect::OpenAi,
    p::claude_responses::stream::ResponsesToClaudeContext {
        usage: Some(initial()),
        restoration: None
    }
);
case!(
    responses_via_claude,
    ResponsesViaClaude,
    Dialect::OpenAi,
    Dialect::Claude,
    ResponsesViaClaudeStreamFacts {
        request: Default::default(),
        response: claude_response_context()
    }
);
case!(
    gemini_via_responses,
    GeminiViaResponses,
    Dialect::Gemini,
    Dialect::OpenAi,
    p::gemini_responses::stream::ResponsesToGeminiContext::default()
);
case!(
    responses_via_gemini,
    ResponsesViaGemini,
    Dialect::OpenAi,
    Dialect::Gemini,
    ResponsesViaGeminiStreamFacts {
        request: Default::default(),
        response: gemini_response_context()
    }
);
