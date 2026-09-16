use super::*;
use crate::{
    Dialect,
    capability::{CapabilityError, CapabilityFuture, CapabilityLimits, StateEntry},
    transform::identity::{IdNamespace, IdentityTarget, SourceIdentity, TargetIdPolicy},
};
use std::{
    future::Future,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
    time::{Duration, UNIX_EPOCH},
};

#[derive(Default)]
struct Store {
    entries: Mutex<BTreeMap<String, StateEntry>>,
    serial: AtomicUsize,
    applied_pending: AtomicBool,
    absent_pending: AtomicBool,
}

impl StateStore for Store {
    type Scope = ();
    fn get<'a>(
        &'a self,
        _: &'a (),
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        Box::pin(async move { Ok(self.entries.lock().unwrap().get(key).cloned()) })
    }
    fn compare_exchange<'a>(
        &'a self,
        _: &'a (),
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async move {
            let serial = self.serial.fetch_add(1, Ordering::SeqCst) + 1;
            if self.absent_pending.swap(false, Ordering::SeqCst) {
                return std::future::pending().await;
            }
            let version = Version::from_bytes(serial.to_be_bytes().to_vec());
            {
                let mut entries = self.entries.lock().unwrap();
                if entries.get(key).map(|v| &v.version) != expected.as_ref() {
                    return Ok(CasResult::Conflict);
                }
                let replacement = replacement.unwrap();
                entries.insert(
                    key.into(),
                    StateEntry {
                        payload: replacement.payload,
                        version: version.clone(),
                        expires_at: replacement.expires_at,
                    },
                );
            }
            if self.applied_pending.swap(false, Ordering::SeqCst) {
                return std::future::pending().await;
            }
            Ok(CasResult::Applied(Some(version)))
        })
    }
    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: Duration::from_secs(30),
            stream_idle: Duration::from_secs(5),
            read_bytes: 65536,
            write_bytes: 65536,
            ws_frame_bytes: 65536,
        }
    }
}

fn state(store: &Store) -> GenerationStateAccess<'_, Store> {
    GenerationStateAccess {
        store,
        scope: &(),
        target: IdentityTarget::new("m", Dialect::Gemini)
            .unwrap()
            .with_origin("origin")
            .unwrap(),
        conversation_key: "conversation".into(),
        now: UNIX_EPOCH,
        expires_at: UNIX_EPOCH + Duration::from_secs(1000),
        max_records: 16,
    }
}

fn ready<F: Future>(future: F) -> F::Output {
    match Box::pin(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending"),
    }
}

fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([91; 16]))
}

fn response(flow: &mut IdentityFlow) -> crate::transform::identity::IdentityHandle {
    flow.resolve_or_allocate(
        IdentityRole::Response,
        SourceIdentity::new(Dialect::Gemini, None, 0),
        &TargetIdPolicy::new(Dialect::OpenAiChat),
    )
    .unwrap()
}

#[test]
fn late_identity_and_complete_tool_name_are_recoverable_without_changing_alias() {
    let host = Store::default();
    let access = state(&host);
    let mut flow = flow();
    let mut ledger = StreamLedger::default();
    let response = response(&mut flow);
    let tool = flow
        .resolve_or_allocate(
            IdentityRole::ToolCall,
            SourceIdentity::new(Dialect::Gemini, None, 0),
            &TargetIdPolicy::new(Dialect::OpenAiChat),
        )
        .unwrap();
    ledger
        .observe_tools(vec![ToolDeclaration {
            id: tool.emitted_id.clone(),
            kind: ToolCallKind::Function,
            name: None,
        }])
        .unwrap();
    ready(ledger.save(&flow, &access, None)).unwrap();
    assert!(
        ready(access.read(IdentityRole::ToolCall, &tool.emitted_id))
            .unwrap()
            .unwrap()
            .tool_name
            .is_none()
    );
    flow.attach_source(&tool, "native-call").unwrap();
    flow.attach_source(&response, "native-response").unwrap();
    ledger
        .observe_tools(vec![ToolDeclaration {
            id: tool.emitted_id.clone(),
            kind: ToolCallKind::Function,
            name: Some("same".into()),
        }])
        .unwrap();
    ready(ledger.save(&flow, &access, None)).unwrap();
    let replay =
        ready(access.recover_tools(std::slice::from_ref(&tool.emitted_id), &BTreeMap::new()))
            .unwrap();
    assert_eq!(replay.names[&tool.emitted_id], "same");
    assert_eq!(replay.original_call_ids[&tool.emitted_id], "native-call");
    let saved = ready(access.read(IdentityRole::ToolCall, &tool.emitted_id))
        .unwrap()
        .unwrap();
    assert_eq!(saved.response_id.as_deref(), Some("native-response"));
    let writes = host.serial.load(Ordering::SeqCst);
    ready(ledger.save(&flow, &access, None)).unwrap();
    assert_eq!(writes, host.serial.load(Ordering::SeqCst));
}

#[test]
fn applied_but_unacknowledged_cas_is_read_back_without_a_second_write() {
    let host = Store::default();
    host.applied_pending.store(true, Ordering::SeqCst);
    let access = state(&host);
    let mut flow = flow();
    response(&mut flow);
    let mut ledger = StreamLedger::default();
    {
        let mut save = Box::pin(ledger.save(&flow, &access, None));
        assert!(
            save.as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }
    assert_eq!(host.serial.load(Ordering::SeqCst), 1);
    ready(ledger.save(&flow, &access, None)).unwrap();
    assert_eq!(host.serial.load(Ordering::SeqCst), 1);
}

#[test]
fn absent_unacknowledged_cas_is_not_retried_and_expired_or_changed_state_blocks_yield() {
    let host = Store::default();
    host.absent_pending.store(true, Ordering::SeqCst);
    let access = state(&host);
    let mut flow = flow();
    response(&mut flow);
    let mut ledger = StreamLedger::default();
    {
        let mut save = Box::pin(ledger.save(&flow, &access, None));
        assert!(
            save.as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }
    assert_eq!(
        ready(ledger.save(&flow, &access, None)).unwrap_err().kind(),
        TransformErrorKind::MissingState
    );
    assert_eq!(host.serial.load(Ordering::SeqCst), 1);
    let host = Store::default();
    let access = state(&host);
    let mut ledger = StreamLedger::default();
    ready(ledger.save(&flow, &access, None)).unwrap();
    host.entries.lock().unwrap().clear();
    assert_eq!(
        ready(ledger.save(&flow, &access, None)).unwrap_err().kind(),
        TransformErrorKind::MissingState
    );
    let mut expired = state(&host);
    expired.now = expired.expires_at;
    assert!(ready(ledger.save(&flow, &expired, None)).is_err());
}

fn chat_responses_pair() -> (
    crate::adapt::generate::stream::event::Collected<
        crate::wire::openai::chat::GenerateContentResponseBody,
    >,
    crate::wire::openai::responses::GenerateContentResponseBody,
    IdentityFlow,
) {
    use crate::{
        adapt::generate::stream::event::{EventLimits, NativeEvent},
        transform::generate::{
            chat_responses::{ResponsesResponseContext, stream::ChatToResponsesStream},
            stream::responses::ResponsesStreamCollector,
        },
        wire::openai::{chat::stream::ChatCompletionChunk, responses as r},
    };
    let chunk:ChatCompletionChunk=serde_json::from_value(serde_json::json!({"id":"actual-response","object":"chat.completion.chunk","created":7,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"actual-call","type":"function","function":{"name":"same","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4,"prompt_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"completion_tokens_details":{"reasoning_tokens":0}}})).unwrap();
    let mut native = ChatCompletionChunk::collector(
        flow(),
        TargetIdPolicy::new(Dialect::OpenAiChat),
        EventLimits {
            max_events: 100,
            max_bytes: 65536,
            max_pending_bytes: 65536,
            max_items: 16,
            max_tools: 16,
            max_parts: 16,
            max_choices: 1,
        },
    );
    ChatCompletionChunk::collect(&mut native, chunk.clone()).unwrap();
    ChatCompletionChunk::collect_done(&mut native).unwrap();
    let native = ChatCompletionChunk::collected(native).unwrap().value;
    let context = ResponsesResponseContext {
        request: r::GenerateContentRequestBody::builder().model("m").build(),
        effective_parallel_tool_calls: true,
        effective_tool_choice: r::ToolChoice::Mode(r::ToolChoiceMode::Auto),
        usage: Default::default(),
        effective_prompt_cache_options: None,
    };
    let mut stream = ChatToResponsesStream::new(context, flow(), Default::default());
    let mut events = stream.push(chunk).unwrap().value;
    stream.push_done().unwrap();
    let end = stream.finish().unwrap();
    events.extend(end.chunks);
    let mut target = ResponsesStreamCollector::new(Default::default());
    for event in events {
        target.push(event).unwrap();
    }
    (native, target.finish().unwrap().value, end.identities)
}

#[test]
fn completed_item_state_keeps_the_provisional_source_call_role() {
    use crate::{
        adapt::generate::GenerationProgress, transform::identity::OutputItemKind,
        wire::openai::responses::ResponseOutputItem,
    };
    let store = Store::default();
    let mut access = state(&store);
    access.target = IdentityTarget::new("m", Dialect::OpenAiChat)
        .unwrap()
        .with_origin("origin")
        .unwrap();
    let (native, client, flow) = chat_responses_pair();
    let ResponseOutputItem::FunctionCall(call) = &client.output[0] else {
        unreachable!()
    };
    let item = call.id.as_ref().unwrap();
    let mut ledger = StreamLedger::default();
    ledger
        .observe_tools(vec![ToolDeclaration {
            id: call.call_id.clone(),
            kind: ToolCallKind::Function,
            name: Some(call.name.clone()),
        }])
        .unwrap();
    ready(ledger.save(&flow, &access, None)).unwrap();
    let role = IdentityRole::OutputItem(OutputItemKind::FunctionCall);
    let before = ready(access.read(role, item)).unwrap().unwrap();
    assert_eq!(before.original_call_id.as_deref(), Some("actual-call"));
    assert!(before.original_item_id.is_none());
    ready(access.save_pair(&native, &client, &flow, &mut GenerationProgress::default())).unwrap();
    let after = ready(access.read(role, item)).unwrap().unwrap();
    assert_eq!(after.original_call_id, before.original_call_id);
    assert!(after.original_item_id.is_none());
    assert_eq!(after.response_id.as_deref(), Some("actual-response"));
}

#[test]
fn final_form_state_recovers_unacknowledged_cas_by_readback_without_repeating_it() {
    use crate::adapt::generate::{ChatCallForm, GenerationProgress};
    let store = Store::default();
    let mut access = state(&store);
    access.target = IdentityTarget::new("m", Dialect::OpenAiChat)
        .unwrap()
        .with_origin("origin")
        .unwrap();
    let (native, client, flow) = chat_responses_pair();
    let mut progress = GenerationProgress::default();
    store.applied_pending.store(true, Ordering::SeqCst);
    {
        let mut future = Box::pin(access.save_pair(&native, &client, &flow, &mut progress));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }
    assert_eq!(store.serial.load(Ordering::SeqCst), 1);
    assert!(progress.pending_identity_write.is_some());
    ready(access.save_pair(&native, &client, &flow, &mut progress)).unwrap();
    assert!(progress.pending_identity_write.is_none());
    assert_eq!(store.serial.load(Ordering::SeqCst), 3);
    let replay = ready(access.recover_tools(&["actual-call".into()], &BTreeMap::new())).unwrap();
    assert_eq!(replay.chat_forms["actual-call"], ChatCallForm::Modern);
    assert_eq!(replay.original_call_ids["actual-call"], "actual-call");
}

#[test]
fn unattributed_responses_call_record_cannot_replay_its_client_alias() {
    let store = Store::default();
    let mut access = state(&store);
    access.target = IdentityTarget::new("m", Dialect::OpenAi)
        .unwrap()
        .with_origin("origin")
        .unwrap();
    let mut flow = flow();
    let call = flow
        .resolve_or_allocate(
            IdentityRole::ToolCall,
            SourceIdentity::new(Dialect::OpenAi, None, 0),
            &TargetIdPolicy::new(Dialect::Claude),
        )
        .unwrap();
    let mut ledger = StreamLedger::default();
    ledger
        .observe_tools(vec![ToolDeclaration {
            id: call.emitted_id.clone(),
            kind: ToolCallKind::Function,
            name: Some("same".into()),
        }])
        .unwrap();
    ready(ledger.save(&flow, &access, None)).unwrap();
    let error =
        ready(access.recover_tools(std::slice::from_ref(&call.emitted_id), &BTreeMap::new()))
            .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::MissingState);
    assert!(
        ready(access.read(IdentityRole::ToolCall, &call.emitted_id))
            .unwrap()
            .unwrap()
            .original_call_id
            .is_none()
    );
}
