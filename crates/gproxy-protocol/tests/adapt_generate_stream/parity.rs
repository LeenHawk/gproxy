use super::*;
use gproxy_protocol::{
    HttpBody,
    adapt::generate::chat_gemini::ChatViaGemini,
    capability::*,
    transform::generate::{gemini_chat::stream::GeminiToChatContext, stream as native},
    wire::{claude::generate_content as c, gemini as g},
};
use std::sync::Mutex;
struct Resources {
    reads: Mutex<usize>,
    bytes: bytes::Bytes,
}
impl Resources {
    fn new() -> Self {
        use base64::Engine;
        Self { reads: Mutex::new(0), bytes: base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC").unwrap().into() }
    }
}
impl ResourceAccess for Resources {
    type Scope = String;
    type PublishedHandle = ();
    fn resolve<'a>(
        &'a self,
        _: &'a String,
        _: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
        panic!("unexpected resolve")
    }
    fn read<'a>(
        &'a self,
        scope: &'a String,
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>> {
        Box::pin(async move {
            assert_eq!(scope, "source-client");
            assert!(matches!(reference, ResourceReference::Url(_)));
            *self.reads.lock().unwrap() += 1;
            Ok(ResourceRead {
                metadata: ResourceMetadata {
                    mime: Some("image/png".into()),
                    length: Some(self.bytes.len() as u64),
                    filename: Some("actual.png".into()),
                    expires_at: None,
                },
                body: HttpBody::Bytes(self.bytes.clone()),
            })
        })
    }
    fn publish<'a>(
        &'a self,
        _: &'a String,
        _: &'a str,
        _: PublicationKind,
        _: ResourceMetadata,
        _: HttpBody,
    ) -> CapabilityFuture<'a, Result<PublishedResource<()>, CapabilityError>> {
        panic!("unexpected publish")
    }
    fn publication_status<'a>(
        &'a self,
        _: &'a String,
        _: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<()>, CapabilityError>> {
        panic!("unexpected status")
    }
    fn release<'a>(
        &'a self,
        _: &'a String,
        _: &'a (),
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        panic!("unexpected release")
    }
    fn limits(&self) -> CapabilityLimits {
        host::limits()
    }
}
fn target(dialect: Dialect, second: bool) -> StreamTarget {
    StreamTarget {
        model: "selected".into(),
        endpoint: Endpoint::new("/selected/generate").unwrap(),
        identities: GenerationIdentity::new(
            IdNamespace([if second { 3 } else { 1 }; 16]),
            IdNamespace([if second { 4 } else { 2 }; 16]),
            Dialect::OpenAiChat,
            dialect,
        )
        .unwrap(),
    }
}
fn access(store: &Store, dialect: Dialect) -> GenerationStateAccess<'_, Store> {
    let mut value = all_pairs::access(store, dialect);
    value.target.origin = Some("actual-upstream".into());
    value
}
fn input() -> h::GenerateContentRequestBody {
    serde_json::from_value(json!({"model":"client","max_completion_tokens":64,"stream":true,"stream_options":{"include_usage":true},"messages":[{"role":"user","content":"hi"}]})).unwrap()
}
fn facts() -> ChatViaGeminiStreamFacts {
    ChatViaGeminiStreamFacts {
        function_names: Default::default(),
        response: GeminiToChatContext {
            created: 123,
            model: Some("selected".into()),
        },
    }
}
fn feed_g(feed: &Feed, body: Value) {
    let body: g::GenerateContentResponseBody = serde_json::from_value(body).unwrap();
    for event in native::gemini::synthesize_gemini_stream(body, Default::default())
        .unwrap()
        .value
    {
        feed.push(format!(
            "data: {}\n\n",
            serde_json::to_string(&event).unwrap()
        ));
    }
    feed.close();
}
fn complete_g(store: Arc<Store>, body: Value) -> Value {
    let feed = Feed::default();
    feed_g(&feed, body);
    let host = Host::stream(store.clone(), feed);
    let access = access(&store, Dialect::Gemini);
    let mut call = ready(ChatViaGemini::prepare_stream(
        input(),
        target(Dialect::Gemini, false),
        facts(),
        settings(),
        &access,
    ))
    .unwrap();
    ready(call.start(&host, &(), &access)).unwrap();
    let GenerationOutcome::Success { response, .. } = ready(call.collect(&access)).unwrap() else {
        panic!("rejected")
    };
    serde_json::to_value(response.body).unwrap()
}
#[test]
fn identical_historical_fixtures_run_through_streaming_post_state_and_resource_capabilities() {
    let cases: Value = serde_json::from_str(include_str!(
        "../fixtures/generation_invocation_parity.json"
    ))
    .unwrap();
    let prior: Value = serde_json::from_str(include_str!(
        "../fixtures/generation_invocation_parity_results.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let store = Arc::new(Store::default());
        let mut reads = 0;
        let value = if case["operation"] == "response" && case["upstream"] == "gemini" {
            complete_g(store.clone(), case["input"].clone())
        } else if case["operation"] == "response" {
            let feed = Feed::default();
            let body: c::GenerateContentResponseBody =
                serde_json::from_value(case["input"].clone()).unwrap();
            for value in native::claude::synthesize_claude_stream(body, Default::default())
                .unwrap()
                .value
            {
                event(&feed, serde_json::to_value(value).unwrap());
            }
            feed.close();
            let host = Host::stream(store.clone(), feed);
            let access = access(&store, Dialect::Claude);
            let mut call = ready(ChatViaClaude::prepare_stream(
                input(),
                target(Dialect::Claude, false),
                ClaudeToChatContext { created: 123 },
                settings(),
                &access,
            ))
            .unwrap();
            ready(call.start(&host, &(), &access)).unwrap();
            let GenerationOutcome::Success { response, .. } = ready(call.collect(&access)).unwrap()
            else {
                panic!("rejected")
            };
            serde_json::to_value(response.body).unwrap()
        } else {
            if let Some(prime) = case.get("prime") {
                complete_g(store.clone(), prime.clone());
            }
            let resources = Resources::new();
            let scope = "source-client".into();
            let resource_context = GenerationResources {
                access: &resources,
                scope: &scope,
                limits: settings().codec,
                max_references: 8,
                now: UNIX_EPOCH,
            };
            let access = access(&store, Dialect::Gemini);
            let mut call = ready(ChatViaGemini::prepare_stream_with_capabilities(
                serde_json::from_value(case["input"].clone()).unwrap(),
                target(Dialect::Gemini, case.get("prime").is_some()),
                facts(),
                settings(),
                &access,
                &resource_context,
            ))
            .unwrap();
            let feed = Feed::default();
            feed_g(
                &feed,
                json!({"responseId":"actual-next-response","modelVersion":"selected","candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"answer"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":4,"candidatesTokenCount":2,"totalTokenCount":6,"cachedContentTokenCount":0,"thoughtsTokenCount":0}}),
            );
            let host = Host::stream(store.clone(), feed);
            ready(call.start(&host, &(), &access)).unwrap();
            ready(call.collect(&access)).unwrap();
            reads = *resources.reads.lock().unwrap();
            let sent = host.sent.lock().unwrap();
            assert_eq!(sent.len(), 1);
            serde_json::from_slice(&sent[0].body).unwrap()
        };
        let expected = prior["v4"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == case["name"])
            .unwrap();
        assert_eq!(
            value, expected["output"],
            "stream result differs for {}",
            case["name"]
        );
        assert_eq!(reads, expected["resource_reads"].as_u64().unwrap() as usize);
        println!(
            "GENERATION_STREAM_PARITY {}",
            json!({"name":case["name"], "accepted":true, "output":value, "resource_reads":reads, "saved_records":store.entries.lock().unwrap().len()})
        );
    }
}
