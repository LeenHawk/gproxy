#![cfg(feature = "custom")]

use gproxy_channel::{
    BaseChannel, ChannelError,
    channel::{
        CredentialView, NormalizedUsage, PrepareContext, ProviderView, ResponseView,
        UsageCompleteness, UsageContext,
    },
    channels::custom::Custom,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};

fn provider<'a>(config: &'a Value, base_url: Option<&'a str>) -> ProviderView<'a> {
    ProviderView {
        id: "p",
        channel: "custom",
        base_url,
        config,
    }
}
fn credential(secret: &Value) -> CredentialView<'_> {
    CredentialView {
        id: "c",
        provider_id: "p",
        auth_kind: "api_key",
        secret,
        metadata: &serde_json::Value::Null,
        version: 0,
        expires_at_ms: None,
    }
}
fn request(path: &str, query: Option<&str>) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer client-secret"),
    );
    headers.insert("x-api-key", HeaderValue::from_static("client-secret"));
    headers.insert("host", HeaderValue::from_static("gproxy.local"));
    headers.insert("content-length", HeaderValue::from_static("2"));
    headers.insert("anthropic-beta", HeaderValue::from_static("files-api"));
    headers.append("x-multi", HeaderValue::from_static("1"));
    headers.append("x-multi", HeaderValue::from_static("2"));
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: query.map(str::to_owned),
        headers,
        body: HttpBody::Bytes(Bytes::from_static(b"{}")),
    }
}
fn prepare<'a>(
    config: &'a Value,
    base_url: Option<&'a str>,
    secret: &'a Value,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
    endpoint_override: Option<&'a str>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    Custom.prepare(PrepareContext {
        provider: provider(config, base_url),
        credential: credential(secret),
        operation: OperationKey {
            operation: Operation::GenerateContent,
            dialect,
        },
        request,
        endpoint_override,
    })
}

#[test]
fn joins_base_url_and_path_and_authenticates_per_family() {
    let config = json!({});
    let secret = json!({"api_key": "sk-upstream"});
    let openai = prepare(
        &config,
        Some("https://api.example/v1/"),
        &secret,
        Dialect::OpenAi,
        request("/v1/responses", Some("key=leak&stream=true&access_token=x")),
        None,
    )
    .unwrap();
    assert_eq!(
        openai.uri(),
        "https://api.example/v1/v1/responses?stream=true"
    );
    assert_eq!(openai.headers()["authorization"], "Bearer sk-upstream");
    assert!(openai.headers().get("x-api-key").is_none());
    assert!(openai.headers().get("host").is_none());
    assert!(openai.headers().get("content-length").is_none());
    assert_eq!(openai.headers()["anthropic-beta"], "files-api");
    assert_eq!(openai.headers().get_all("x-multi").iter().count(), 2);

    let claude = prepare(
        &config,
        Some("https://claude.example"),
        &secret,
        Dialect::Claude,
        request("v1/messages", None),
        None,
    )
    .unwrap();
    assert_eq!(claude.uri(), "https://claude.example/v1/messages");
    assert_eq!(claude.headers()["x-api-key"], "sk-upstream");
    assert_eq!(claude.headers()["anthropic-version"], "2023-06-01");
    assert!(claude.headers().get("authorization").is_none());

    let gemini = prepare(
        &config,
        Some("https://g.example"),
        &secret,
        Dialect::Gemini,
        request("/v1beta/models/x:generateContent", None),
        Some("https://override.example/gen"),
    )
    .unwrap();
    assert_eq!(gemini.uri(), "https://override.example/gen");
    assert_eq!(gemini.headers()["x-goog-api-key"], "sk-upstream");
}

#[test]
fn config_overrides_auth_and_adds_headers_and_declares_dialects() {
    let config = json!({
        "dialects": ["openai_chat"],
        "auth_header": "api-key",
        "headers": {"x-vendor": "yes"}
    });
    let secret = json!({"api_key": "k"});
    let prepared = prepare(
        &config,
        Some("https://azure.example"),
        &secret,
        Dialect::OpenAiChat,
        request(
            "/openai/deployments/x/chat/completions",
            Some("api-version=2024"),
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        prepared.uri(),
        "https://azure.example/openai/deployments/x/chat/completions?api-version=2024"
    );
    assert_eq!(prepared.headers()["api-key"], "k");
    assert!(prepared.headers().get("authorization").is_none());
    assert_eq!(prepared.headers()["x-vendor"], "yes");
    assert_eq!(
        Custom.native_dialects(provider(&config, None), Operation::GenerateContent),
        [Dialect::OpenAiChat]
    );
    assert_eq!(
        Custom
            .native_dialects(provider(&json!({}), None), Operation::ListModels)
            .len(),
        4
    );

    assert!(matches!(
        prepare(
            &json!({}),
            None,
            &secret,
            Dialect::OpenAi,
            request("/x", None),
            None
        ),
        Err(ChannelError::InvalidConfig(_))
    ));
    assert!(matches!(
        prepare(
            &json!({}),
            Some("https://a"),
            &json!({}),
            Dialect::OpenAi,
            request("/x", None),
            None
        ),
        Err(ChannelError::InvalidCredential)
    ));
    let ws = Custom
        .prepare_connect(PrepareContext {
            provider: provider(&json!({}), Some("https://rt.example")),
            credential: credential(&secret),
            operation: OperationKey {
                operation: Operation::ConnectRealtime,
                dialect: Dialect::OpenAi,
            },
            request: WireRequest {
                method: Method::GET,
                path: "/v1/realtime".into(),
                query: Some("model=gpt-realtime".into()),
                headers: HeaderMap::new(),
                body: (),
            },
            endpoint_override: None,
        })
        .unwrap();
    assert_eq!(
        ws.uri(),
        "https://rt.example/v1/realtime?model=gpt-realtime"
    );
    assert_eq!(ws.headers()["authorization"], "Bearer k");
}

#[test]
fn allowed_headers_restricts_forwarding_to_the_named_set() {
    let config = json!({"allowed_headers": ["x-multi", "Anthropic-Beta"]});
    let secret = json!({"api_key": "sk"});
    let mut extra = request("/v1/messages", None);
    extra
        .headers
        .insert("x-leaky", HeaderValue::from_static("secret"));
    extra
        .headers
        .insert("content-type", HeaderValue::from_static("application/json"));
    let prepared = prepare(
        &config,
        Some("https://api.example"),
        &secret,
        Dialect::Claude,
        extra,
        None,
    )
    .unwrap();
    let headers = prepared.headers();
    assert!(headers.get("x-leaky").is_none(), "not on the list");
    assert_eq!(
        headers.get_all("x-multi").iter().count(),
        2,
        "listed, repeats kept"
    );
    assert_eq!(
        headers["anthropic-beta"], "files-api",
        "names match case-insensitively"
    );
    assert_eq!(
        headers["content-type"], "application/json",
        "always forwarded"
    );
    assert_eq!(
        headers["x-api-key"], "sk",
        "channel-injected headers are not subject to the list"
    );
    assert!(headers.get("anthropic-version").is_some());

    let bad = json!({"allowed_headers": ["not a header"]});
    assert!(matches!(
        prepare(
            &bad,
            Some("https://api.example"),
            &secret,
            Dialect::Claude,
            request("/v1/messages", None),
            None
        ),
        Err(ChannelError::InvalidConfig(_))
    ));
}

// ----------------------------------------------------------- magic cache

const MAGIC_AUTO: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";
const MAGIC_5M: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF";
const MAGIC_PREFIX: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_";

#[test]
fn magic_cache_strings_follow_the_operation_dialect() {
    let secret = json!({"api_key": "sk-upstream"});
    let shaped = |config: &Value, dialect: Dialect, body: &[u8]| -> Vec<u8> {
        let mut request = request("/v1/x", None);
        request.body = HttpBody::Bytes(Bytes::copy_from_slice(body));
        let prepared = prepare(
            config,
            Some("https://api.example"),
            &secret,
            dialect,
            request,
            None,
        )
        .unwrap();
        let HttpBody::Bytes(bytes) = prepared.into_body() else {
            panic!("buffered");
        };
        bytes.to_vec()
    };
    let enabled = json!({"enable_claude_magic_cache": true, "enable_openai_magic_cache": true});
    let disabled = json!({});
    let parse = |bytes: Vec<u8>| serde_json::from_slice::<Value>(&bytes).unwrap();

    let claude = format!(r#"{{"messages":[{{"role":"user","content":"ctx {MAGIC_5M}"}}]}}"#);
    assert_eq!(
        parse(shaped(&enabled, Dialect::Claude, claude.as_bytes()))["messages"][0]["content"],
        json!([{"type": "text", "text": "ctx", "cache_control": {"type": "ephemeral", "ttl": "5m"}}])
    );
    let chat = format!(
        r#"{{"messages":[{{"role":"user","content":"ctx {MAGIC_AUTO}"}},{{"role":"user","content":[{{"type":"text","text":"pinned","prompt_cache_breakpoint":{{"mode":"explicit"}}}}]}}]}}"#
    );
    assert_eq!(
        parse(shaped(&enabled, Dialect::OpenAiChat, chat.as_bytes()))["messages"][0]["content"],
        json!([{"type": "text", "text": "ctx ", "prompt_cache_breakpoint": {"mode": "explicit"}}])
    );
    let responses = format!(r#"{{"input":"ctx {MAGIC_AUTO}"}}"#);
    assert_eq!(
        parse(shaped(&enabled, Dialect::OpenAi, responses.as_bytes()))["input"],
        json!([{"type": "message", "role": "user", "content": [
            {"type": "input_text", "text": "ctx ", "prompt_cache_breakpoint": {"mode": "explicit"}}
        ]}])
    );
    let gemini = format!(r#"{{"contents":[{{"parts":[{{"text":"ctx {MAGIC_AUTO}"}}]}}]}}"#);
    assert_eq!(
        parse(shaped(&enabled, Dialect::Gemini, gemini.as_bytes())),
        json!({"contents": [{"parts": [{"text": "ctx "}]}]}),
        "Gemini has no breakpoint; the token is only stripped"
    );

    let bytes = shaped(&disabled, Dialect::OpenAiChat, chat.as_bytes());
    assert!(!String::from_utf8_lossy(&bytes).contains(MAGIC_PREFIX));
    assert_eq!(
        bytes,
        serde_json::to_vec(&json!({"messages": [
            {"role": "user", "content": "ctx "},
            {"role": "user", "content": [
                {"type": "text", "text": "pinned", "prompt_cache_breakpoint": {"mode": "explicit"}}
            ]}
        ]}))
        .unwrap(),
        "disabled: stripped, the client's breakpoint stays"
    );
    assert_eq!(
        parse(shaped(&disabled, Dialect::Claude, claude.as_bytes())),
        json!({"messages": [{"role": "user", "content": "ctx "}]}),
        "disabled: no canonicalization"
    );
    let plain = br#"{"input":  "x", "prompt_cache_breakpoint": {"mode": "explicit"}}"#;
    assert_eq!(shaped(&enabled, Dialect::OpenAi, plain), plain.to_vec());
    assert_eq!(shaped(&disabled, Dialect::OpenAi, plain), plain.to_vec());
}

// ----------------------------------------------------------------- usage

/// What the channel reads out of one reply, or `None` when it reads nothing.
fn usage(
    operation: Operation,
    dialect: Dialect,
    status: StatusCode,
    body: &[u8],
) -> Option<NormalizedUsage> {
    let extractor = Custom
        .usage_extractor()
        .expect("custom meters what the upstream reports");
    let headers = HeaderMap::new();
    extractor
        .extract(UsageContext {
            operation: OperationKey { operation, dialect },
            request_body: None,
            response: ResponseView {
                status,
                headers: &headers,
                body,
            },
        })
        .unwrap()
}

fn generated(dialect: Dialect, body: &str) -> Option<NormalizedUsage> {
    usage(
        Operation::GenerateContent,
        dialect,
        StatusCode::OK,
        body.as_bytes(),
    )
}

#[test]
fn a_buffered_reply_is_metered_from_the_upstreams_own_usage_block() {
    let chat = generated(
        Dialect::OpenAiChat,
        r#"{"choices":[{"message":{"content":"hi"}}],
            "usage":{"prompt_tokens":150,"completion_tokens":40,
                "prompt_tokens_details":{"cached_tokens":50},
                "completion_tokens_details":{"reasoning_tokens":12}}}"#,
    )
    .expect("chat completions usage");
    assert_eq!(
        chat.tokens.input_tokens,
        Some(100),
        "prompt_tokens counts cache reads; the normalized input does not"
    );
    assert_eq!(chat.tokens.output_tokens, Some(40));
    assert_eq!(chat.tokens.cached_input_tokens, Some(50));
    assert_eq!(chat.tokens.reasoning_tokens, Some(12));
    assert_eq!(chat.completeness, UsageCompleteness::Complete);

    let responses = generated(
        Dialect::OpenAi,
        r#"{"usage":{"input_tokens":120,"output_tokens":30,
            "input_tokens_details":{"cached_tokens":20},
            "output_tokens_details":{"reasoning_tokens":10}}}"#,
    )
    .expect("responses usage");
    assert_eq!(responses.tokens.input_tokens, Some(100));
    assert_eq!(responses.tokens.output_tokens, Some(30));
    assert_eq!(responses.tokens.cached_input_tokens, Some(20));
    assert_eq!(responses.tokens.reasoning_tokens, Some(10));

    let claude = generated(
        Dialect::Claude,
        r#"{"usage":{"input_tokens":7,"output_tokens":9,
            "cache_read_input_tokens":4,
            "cache_creation":{"ephemeral_5m_input_tokens":5,
                "ephemeral_1h_input_tokens":6}}}"#,
    )
    .expect("claude usage");
    assert_eq!(
        claude.tokens.input_tokens,
        Some(7),
        "Claude's input already excludes both cache counters"
    );
    assert_eq!(claude.tokens.output_tokens, Some(9));
    assert_eq!(claude.tokens.cached_input_tokens, Some(4));
    assert_eq!(claude.tokens.cache_creation_5m_tokens, Some(5));
    assert_eq!(claude.tokens.cache_creation_1h_tokens, Some(6));

    let gemini = generated(
        Dialect::Gemini,
        r#"{"usageMetadata":{"promptTokenCount":200,"candidatesTokenCount":60,
            "thoughtsTokenCount":25,"cachedContentTokenCount":80,
            "toolUsePromptTokenCount":13}}"#,
    )
    .expect("gemini usage");
    assert_eq!(gemini.tokens.input_tokens, Some(120));
    assert_eq!(
        gemini.tokens.output_tokens,
        Some(85),
        "candidates exclude thoughts; the normalized output includes them"
    );
    assert_eq!(gemini.tokens.cached_input_tokens, Some(80));
    assert_eq!(gemini.tokens.reasoning_tokens, Some(25));
    assert_eq!(
        gemini
            .metrics
            .get("tool_use_prompt_tokens")
            .map(ToString::to_string),
        Some("13".to_owned())
    );
}

#[test]
fn a_streamed_reply_is_metered_from_its_accumulated_events() {
    // Claude splits one response's counts across two events.
    let claude = usage(
        Operation::StreamGenerateContent,
        Dialect::Claude,
        StatusCode::OK,
        concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":11,\"cache_read_input_tokens\":4}}}\n",
            "\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"hi\"}}\n",
            "\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":5}}\n",
            "\n",
        )
        .as_bytes(),
    )
    .expect("streamed claude usage");
    assert_eq!(claude.tokens.input_tokens, Some(11));
    assert_eq!(claude.tokens.output_tokens, Some(5));
    assert_eq!(claude.tokens.cached_input_tokens, Some(4));

    // Chat Completions carries a null usage until its last chunk.
    let chat = usage(
        Operation::StreamGenerateContent,
        Dialect::OpenAiChat,
        StatusCode::OK,
        concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}],\"usage\":null}\n",
            "\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":31,\"completion_tokens\":17,\"prompt_tokens_details\":{\"cached_tokens\":9}}}\n",
            "\n",
            "data: [DONE]\n",
            "\n",
        )
        .as_bytes(),
    )
    .expect("streamed chat usage");
    assert_eq!(chat.tokens.input_tokens, Some(22));
    assert_eq!(chat.tokens.output_tokens, Some(17));
    assert_eq!(chat.tokens.cached_input_tokens, Some(9));

    // Gemini repeats a complete block; the later one supersedes the earlier.
    let gemini = usage(
        Operation::StreamGenerateContent,
        Dialect::Gemini,
        StatusCode::OK,
        concat!(
            "data: {\"usageMetadata\":{\"promptTokenCount\":40,\"candidatesTokenCount\":2}}\n",
            "\n",
            "data: {\"usageMetadata\":{\"promptTokenCount\":40,\"candidatesTokenCount\":18}}\n",
            "\n",
        )
        .as_bytes(),
    )
    .expect("streamed gemini usage");
    assert_eq!(gemini.tokens.input_tokens, Some(40));
    assert_eq!(gemini.tokens.output_tokens, Some(18));

    // Responses names its counts on the terminal event only.
    let responses = usage(
        Operation::StreamGenerateContent,
        Dialect::OpenAi,
        StatusCode::OK,
        concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"usage\":null}}\n",
            "\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":64,\"output_tokens\":28,\"output_tokens_details\":{\"reasoning_tokens\":6}}}}\n",
            "\n",
        )
        .as_bytes(),
    )
    .expect("streamed responses usage");
    assert_eq!(responses.tokens.input_tokens, Some(64));
    assert_eq!(responses.tokens.output_tokens, Some(28));
    assert_eq!(responses.tokens.reasoning_tokens, Some(6));
}

#[test]
fn a_reply_that_reports_nothing_readable_is_left_to_the_hosts_estimate() {
    // Reading nothing is reported as nothing, so the host's own estimate fills
    // the record and marks it. A guess dressed as an upstream count would be
    // worse than an admitted guess.
    assert!(
        generated(
            Dialect::OpenAiChat,
            r#"{"choices":[{"message":{"content":"hi"}}]}"#
        )
        .is_none(),
        "no usage object at all"
    );
    assert!(
        generated(Dialect::OpenAiChat, r#"{"usage":{"prompt_tokens":"many"}}"#).is_none(),
        "counts that are not numbers"
    );
    assert!(
        generated(
            Dialect::OpenAi,
            r#"{"usage":{"prompt_tokens":10,"completion_tokens":2}}"#
        )
        .is_none(),
        "a Chat body from a provider configured for Responses names neither count this reading knows"
    );
    assert!(
        generated(
            Dialect::Claude,
            r#"{"usageMetadata":{"promptTokenCount":9}}"#
        )
        .is_none(),
        "another vendor's shape is not read with Claude's field names"
    );
    assert!(
        generated(Dialect::Gemini, "<html>gateway error</html>").is_none(),
        "neither JSON nor an event stream"
    );

    // A refusal consumed nothing, and neither did an operation whose answer
    // merely contains numbers.
    assert!(
        usage(
            Operation::GenerateContent,
            Dialect::OpenAi,
            StatusCode::TOO_MANY_REQUESTS,
            br#"{"usage":{"input_tokens":1,"output_tokens":1}}"#,
        )
        .is_none(),
        "a rejected call is not metered"
    );
    assert!(
        usage(
            Operation::CountTokens,
            Dialect::Claude,
            StatusCode::OK,
            br#"{"usage":{"input_tokens":512,"output_tokens":0}}"#,
        )
        .is_none(),
        "counting tokens reports no consumption"
    );
    assert!(
        usage(
            Operation::ListModels,
            Dialect::OpenAi,
            StatusCode::OK,
            br#"{"data":[],"usage":{"input_tokens":3,"output_tokens":4}}"#,
        )
        .is_none(),
        "listing models reports no consumption"
    );
}

#[test]
fn every_dialect_a_provider_can_declare_has_a_reading() {
    // `native_dialects` hands back whatever `config.dialects` names, so the
    // reading has to answer for each of them and not for a curated few.
    let config = json!({"dialects": [
        "openai", "openai_chat", "claude", "gemini", "openai_responses_websocket"
    ]});
    let declared = Custom.native_dialects(provider(&config, None), Operation::GenerateContent);
    assert_eq!(declared.len(), 5);
    for dialect in declared {
        let body = match dialect {
            Dialect::OpenAiChat => r#"{"usage":{"prompt_tokens":12,"completion_tokens":3}}"#,
            Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket | Dialect::Claude => {
                r#"{"usage":{"input_tokens":12,"output_tokens":3}}"#
            }
            Dialect::Gemini => {
                r#"{"usageMetadata":{"promptTokenCount":12,"candidatesTokenCount":3}}"#
            }
        };
        let read = generated(dialect, body).unwrap_or_else(|| panic!("{dialect:?} is unread"));
        assert_eq!(read.tokens.input_tokens, Some(12), "{dialect:?}");
        assert_eq!(read.tokens.output_tokens, Some(3), "{dialect:?}");
    }
}
