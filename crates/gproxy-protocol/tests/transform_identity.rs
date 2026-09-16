use gproxy_protocol::Dialect;
use gproxy_protocol::capability::{
    CapabilityError, CapabilityFuture, CapabilityLimits, CasResult, StateEntry, StateStore,
    StateWrite, Version,
};
use gproxy_protocol::transform::identity::*;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime};

fn make_dialect(value: &str) -> DialectId {
    match value {
        "openai" => Dialect::OpenAi,
        "claude" => Dialect::Claude,
        "gemini" => Dialect::Gemini,
        _ => panic!("unknown test dialect"),
    }
}

fn source(dialect_name: &str, id: Option<&str>, index: u64) -> SourceIdentity {
    SourceIdentity::new(make_dialect(dialect_name), id.map(str::to_owned), index)
}

fn target() -> TargetIdPolicy {
    TargetIdPolicy::new(make_dialect("openai"))
}

#[test]
fn namespaces_and_roles_are_unique_and_prefixes_are_exact() {
    let namespace_a = IdNamespace::with_bytes([1; 16]);
    let namespace_b = IdNamespace::with_bytes([2; 16]);
    assert_ne!(namespace_a.hex(), namespace_b.hex());
    assert_eq!(KnownIdPrefix::parse("call_foo"), Some(KnownIdPrefix::Call));
    assert_eq!(KnownIdPrefix::parse("callback"), None);

    let mut a = IdentityFlow::new(namespace_a);
    let mut b = IdentityFlow::new(namespace_b);
    let call = a
        .resolve_or_allocate(IdentityRole::ToolCall, source("gemini", None, 0), &target())
        .unwrap();
    let message = a
        .resolve_or_allocate(IdentityRole::Message, source("gemini", None, 0), &target())
        .unwrap();
    let other = b
        .resolve_or_allocate(IdentityRole::ToolCall, source("gemini", None, 0), &target())
        .unwrap();
    assert!(call.emitted_id.starts_with("call_"));
    assert!(message.emitted_id.starts_with("msg_"));
    assert_ne!(call.emitted_id, message.emitted_id);
    assert_ne!(call.emitted_id, other.emitted_id);
    let changed_target = TargetIdPolicy::new(make_dialect("claude"));
    assert!(matches!(
        a.resolve_or_allocate(
            IdentityRole::Response,
            source("gemini", None, 9),
            &changed_target
        ),
        Err(IdentityError::InvalidIdentity(_))
    ));
}

#[test]
fn same_name_and_missing_ids_use_logical_position() {
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([3; 16]));
    let first = flow
        .resolve_or_allocate(
            IdentityRole::ToolCall,
            source("gemini", Some("same_name"), 0),
            &target(),
        )
        .unwrap();
    let second = flow.resolve_or_allocate(
        IdentityRole::ToolCall,
        source("gemini", Some("same_name"), 1),
        &target(),
    );
    assert!(matches!(
        second,
        Err(IdentityError::DuplicateSourceIdentity { .. })
    ));

    let first_missing = flow
        .resolve_or_allocate(IdentityRole::ToolCall, source("gemini", None, 2), &target())
        .unwrap();
    let second_missing = flow
        .resolve_or_allocate(IdentityRole::ToolCall, source("gemini", None, 3), &target())
        .unwrap();
    assert_ne!(first.emitted_id, first_missing.emitted_id);
    assert_ne!(first_missing.emitted_id, second_missing.emitted_id);
}

#[test]
fn source_ids_are_preserved_when_valid_and_collisions_are_disambiguated() {
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([4; 16]));
    let preserved = flow
        .resolve_or_allocate(
            IdentityRole::ToolCall,
            source("claude", Some("call_original"), 0),
            &target(),
        )
        .unwrap();
    assert_eq!(preserved.emitted_id, "call_original");
    let collision = flow
        .resolve_or_allocate(
            IdentityRole::Message,
            source("claude", Some("call_original"), 0),
            &target(),
        )
        .unwrap();
    assert_eq!(collision.emitted_id, preserved.emitted_id);
    assert_eq!(
        flow.lookup_emitted_as(IdentityRole::ToolCall, "call_original")
            .unwrap()
            .role,
        IdentityRole::ToolCall
    );
    assert_eq!(
        flow.lookup_emitted_as(IdentityRole::Message, "call_original")
            .unwrap()
            .role,
        IdentityRole::Message
    );
}

#[test]
fn late_source_id_does_not_change_emitted_alias_and_links_call_result() {
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([5; 16]));
    let call = flow
        .resolve_or_allocate(IdentityRole::ToolCall, source("gemini", None, 0), &target())
        .unwrap();
    let emitted = call.emitted_id.clone();
    let late = flow.attach_source(&call, "gemini-call-1").unwrap();
    assert_eq!(late.emitted_id, emitted);
    assert_eq!(
        flow.lookup_source(
            &source("gemini", Some("gemini-call-1"), 0),
            IdentityRole::ToolCall
        )
        .unwrap()
        .emitted_id,
        emitted
    );

    let result = flow
        .resolve_or_allocate(
            IdentityRole::OutputItem(OutputItemKind::FunctionCallOutput),
            source("gemini", Some("result-item"), 1),
            &target(),
        )
        .unwrap();
    let link = flow.link_call_result(&late, Some(&result)).unwrap();
    assert_eq!(link.call_id, emitted);
    assert_eq!(link.output_item_id, Some(result.emitted_id));
}

#[test]
fn custom_tool_and_function_output_prefixes_are_explicit() {
    let function_output = IdentityRole::OutputItem(OutputItemKind::FunctionCallOutput);
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([6; 16]));
    assert!(matches!(
        flow.resolve_or_allocate(function_output, source("openai", None, 0), &target()),
        Err(IdentityError::NoGeneratedPrefix)
    ));
    let policy = target()
        .with_generated_prefix(function_output, KnownIdPrefix::FunctionCallOutputItem)
        .with_generated_prefix(
            IdentityRole::OutputItem(OutputItemKind::CustomToolCall),
            KnownIdPrefix::CallToolCall,
        );
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([6; 16]));
    let output = flow
        .resolve_or_allocate(function_output, source("openai", None, 0), &policy)
        .unwrap();
    assert!(output.emitted_id.starts_with("fco_"));
}

#[derive(Clone, Default)]
struct FakeStore {
    entries: Arc<Mutex<HashMap<(String, String), StateEntry>>>,
    next_version: Arc<Mutex<u64>>,
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut context = Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fake store unexpectedly pending"),
    }
}

impl StateStore for FakeStore {
    type Scope = String;

    fn get<'a>(
        &'a self,
        scope: &'a Self::Scope,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        let mut entries = self.entries.lock().unwrap();
        let key = (scope.clone(), key.to_owned());
        if entries
            .get(&key)
            .is_some_and(|e| e.expires_at.is_some_and(|t| t <= SystemTime::now()))
        {
            entries.remove(&key);
        }
        let entry = entries.get(&key).cloned();
        Box::pin(async move { Ok(entry) })
    }

    fn compare_exchange<'a>(
        &'a self,
        scope: &'a Self::Scope,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        let map_key = (scope.clone(), key.to_owned());
        let mut entries = self.entries.lock().unwrap();
        if entries
            .get(&map_key)
            .is_some_and(|e| e.expires_at.is_some_and(|t| t <= SystemTime::now()))
        {
            entries.remove(&map_key);
        }
        let current = entries.get(&map_key).map(|entry| entry.version.clone());
        let result = if current != expected {
            CasResult::Conflict
        } else if let Some(replacement) = replacement {
            let mut next = self.next_version.lock().unwrap();
            *next += 1;
            let version = Version::from_bytes(next.to_be_bytes().to_vec());
            entries.insert(
                map_key,
                StateEntry {
                    payload: replacement.payload,
                    version: version.clone(),
                    expires_at: replacement.expires_at,
                },
            );
            CasResult::Applied(Some(version))
        } else {
            entries.remove(&map_key);
            CasResult::Applied(None)
        };
        Box::pin(async move { Ok(result) })
    }

    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: Duration::from_secs(1),
            stream_idle: Duration::from_secs(1),
            read_bytes: 64 * 1024,
            write_bytes: 64 * 1024,
            ws_frame_bytes: 64 * 1024,
        }
    }
}

#[test]
fn state_is_scoped_cas_checked_and_late_facts_are_immutable() {
    let store = FakeStore::default();
    let state = IdentityStateStore::new(store);
    let scope = "conversation-a".to_owned();
    let key = "tool-call";
    let target = IdentityTarget::new("model-a", make_dialect("openai"))
        .unwrap()
        .with_origin("provider-a")
        .unwrap();
    let mut record = IdentityStateRecord::new(IdentityRole::ToolCall, target.clone());
    record.client_call_id = Some("call_client".into());
    record.opaque_signature = Some(
        OpaqueSignature::new(
            gproxy_protocol::transform::identity::OpaqueField::ResponsesReasoningEncryptedContent,
            "sig",
            "provider-a",
            "model-a",
        )
        .unwrap(),
    );
    let version = ready(state.save(&scope, key, None, &record, None)).unwrap();
    assert!(matches!(
        ready(state.save(&scope, key, None, &record, None)),
        Err(IdentityError::Conflict)
    ));
    let loaded = ready(state.read(&scope, key)).unwrap();
    let updated = ready(state.update_late(
        &scope,
        key,
        version,
        LateIdentityFacts {
            original_call_id: Some("call_origin".into()),
            ..Default::default()
        },
    ))
    .unwrap();
    assert_eq!(
        updated.record.original_call_id.as_deref(),
        Some("call_origin")
    );
    assert_eq!(updated.record.client_call_id, loaded.record.client_call_id);
    assert!(matches!(
        ready(state.read(&scope, key)),
        Err(IdentityError::InvalidIdentity(_))
    ));
    assert!(matches!(
        ready(state.read(&scope, "missing")),
        Err(IdentityError::MissingState)
    ));
    let expired_key = "expired";
    ready(state.save(
        &scope,
        expired_key,
        None,
        &record,
        Some(SystemTime::now() - Duration::from_secs(1)),
    ))
    .unwrap();
    assert!(matches!(
        ready(state.read(&scope, expired_key)),
        Err(IdentityError::MissingState)
    ));
    assert!(updated.expires_at.is_none());
}

#[test]
fn allocation_is_stable_under_reordering_and_distinguishes_roles_and_dialects() {
    let namespace = IdNamespace::with_bytes([7; 16]);
    let mut first = IdentityFlow::new(namespace);
    let mut second = IdentityFlow::new(namespace);
    let roles = [
        IdentityRole::Message,
        IdentityRole::OutputItem(OutputItemKind::Message),
        IdentityRole::ToolCall,
    ];
    let mut expected = HashMap::new();
    for role in roles {
        for dialect in [Dialect::Claude, Dialect::Gemini] {
            let handle = first
                .resolve_or_allocate(role, SourceIdentity::new(dialect, None, 42), &target())
                .unwrap();
            expected.insert((role, dialect), handle.emitted_id);
        }
    }
    for role in roles.into_iter().rev() {
        for dialect in [Dialect::Gemini, Dialect::Claude] {
            let handle = second
                .resolve_or_allocate(role, SourceIdentity::new(dialect, None, 42), &target())
                .unwrap();
            assert_eq!(handle.emitted_id, expected[&(role, dialect)]);
        }
    }
    let unique = expected.values().collect::<std::collections::HashSet<_>>();
    assert_eq!(unique.len(), 6);
}
#[test]
fn impossible_policy_returns_immediately_and_cross_role_ids_are_reidentified() {
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([8; 16]));
    assert!(
        flow.resolve_or_allocate(
            IdentityRole::ToolCall,
            source("gemini", None, 1),
            &target().with_max_len(3)
        )
        .is_err()
    );
    assert!(
        flow.resolve_or_allocate(
            IdentityRole::ToolCall,
            source("gemini", None, 1),
            &target().with_generated_prefixes([])
        )
        .is_err()
    );
    let role = IdentityRole::OutputItem(OutputItemKind::FunctionCall);
    let handle = flow
        .resolve_as(
            IdentityRole::ToolCall,
            role,
            source("gemini", Some("call_native"), 1),
            &target(),
        )
        .unwrap();
    assert_ne!(handle.emitted_id, "call_native");
    assert!(handle.emitted_id.starts_with("fc_"));
    assert_eq!(handle.source_id(), Some("call_native"));
    assert!(
        flow.resolve_or_allocate(role, source("gemini", Some("call_native"), 1), &target())
            .is_err()
    );
}
#[test]
fn absent_result_item_id_and_foreign_flow_handles_are_not_fabricated() {
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([9; 16]));
    let call = flow
        .resolve_or_allocate(
            IdentityRole::ToolCall,
            source("gemini", Some("native"), 0),
            &target(),
        )
        .unwrap();
    let linked = flow.link_call_result(&call, None).unwrap();
    assert_eq!(linked.call_id, "native");
    assert_eq!(linked.output_item_id, None);
    let mut other = IdentityFlow::new(IdNamespace::with_bytes([10; 16]));
    let foreign = other
        .resolve_or_allocate(
            IdentityRole::ToolCall,
            source("gemini", Some("native"), 0),
            &target(),
        )
        .unwrap();
    assert!(flow.link_call_result(&foreign, None).is_err());
    assert!(flow.attach_source(&foreign, "late").is_err());
}
#[test]
fn unicode_and_long_legal_ids_survive_both_initial_and_late_paths() {
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([11; 16]));
    let id = "操作".repeat(9000);
    let first = flow
        .resolve_or_allocate(
            IdentityRole::ToolCall,
            SourceIdentity::new(Dialect::Gemini, Some(id.clone()), 0),
            &target(),
        )
        .unwrap();
    assert_eq!(first.emitted_id, id);
    let late = flow
        .resolve_or_allocate(IdentityRole::ToolCall, source("gemini", None, 1), &target())
        .unwrap();
    let other = format!("{id}x");
    let attached = flow.attach_source(&late, other.clone()).unwrap();
    assert_eq!(attached.source_id(), Some(other.as_str()));
}
#[test]
fn signatures_and_state_validate_after_deserialization_not_only_construction() {
    let target = IdentityTarget::new("model", Dialect::Gemini)
        .unwrap()
        .with_origin("provider/principal-a")
        .unwrap();
    let mut record = IdentityStateRecord::new(IdentityRole::ToolCall, target.clone());
    record.client_call_id = Some("client".into());
    record.opaque_signature = Some(
        OpaqueSignature::new(
            gproxy_protocol::transform::identity::OpaqueField::GeminiPartThoughtSignature,
            "sig",
            "provider/principal-a",
            "model",
        )
        .unwrap(),
    );
    for (origin, model) in [
        ("provider/principal-b", "model"),
        ("provider/principal-a", "other"),
    ] {
        let _other = IdentityTarget::new(model, Dialect::Gemini)
            .unwrap()
            .with_origin(origin)
            .unwrap();
    }
    let wire = serde_json::to_value(&record).unwrap();
    let mut invalid = wire;
    invalid["opaque_signature"]["value"] = serde_json::json!("");
    let _invalid: IdentityStateRecord = serde_json::from_value(invalid).unwrap();
}
#[derive(Clone)]
struct ErrorStore;
impl StateStore for ErrorStore {
    type Scope = ();
    fn get<'a>(
        &'a self,
        _: &'a (),
        _: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        Box::pin(async {
            Err(CapabilityError::with_source(
                gproxy_protocol::capability::CapabilityErrorKind::Storage,
                gproxy_protocol::capability::CapabilityErrorStage::Start,
                "db read",
                std::io::Error::other("original"),
            ))
        })
    }
    fn compare_exchange<'a>(
        &'a self,
        _: &'a (),
        _: &'a str,
        _: Option<Version>,
        _: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async {
            Err(CapabilityError::with_source(
                gproxy_protocol::capability::CapabilityErrorKind::Storage,
                gproxy_protocol::capability::CapabilityErrorStage::Start,
                "db write",
                std::io::Error::other("original"),
            ))
        })
    }
    fn limits(&self) -> CapabilityLimits {
        FakeStore::default().limits()
    }
}
#[test]
fn state_storage_failures_keep_the_original_error_chain() {
    use std::error::Error;
    let target = IdentityTarget::new("model", Dialect::Gemini).unwrap();
    let record = IdentityStateRecord::new(IdentityRole::ToolCall, target.clone());
    let store = IdentityStateStore::new(ErrorStore);
    for err in [
        ready(store.read(&(), "key")).unwrap_err(),
        ready(store.save(&(), "key", None, &record, None)).unwrap_err(),
    ] {
        let host = err
            .source()
            .unwrap()
            .downcast_ref::<CapabilityError>()
            .unwrap();
        assert!(host.source().unwrap().is::<std::io::Error>());
        assert_eq!(host.source().unwrap().to_string(), "original");
    }
}
#[derive(Clone)]
struct InterleavingStore {
    inner: FakeStore,
    reads: Arc<std::sync::atomic::AtomicUsize>,
    writes: Arc<std::sync::atomic::AtomicUsize>,
}
impl StateStore for InterleavingStore {
    type Scope = String;
    fn get<'a>(
        &'a self,
        s: &'a String,
        k: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.get(s, k)
    }
    fn compare_exchange<'a>(
        &'a self,
        s: &'a String,
        k: &'a str,
        v: Option<Version>,
        w: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        let result = ready(self.inner.compare_exchange(s, k, v, w));
        if self
            .writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            == 1
        {
            self.inner
                .entries
                .lock()
                .unwrap()
                .remove(&(s.clone(), k.to_owned()));
        }
        Box::pin(async move { result })
    }
    fn limits(&self) -> CapabilityLimits {
        self.inner.limits()
    }
}
#[test]
fn late_cas_returns_its_own_committed_snapshot_without_racing_a_second_read() {
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let store = IdentityStateStore::new(InterleavingStore {
        inner: FakeStore::default(),
        reads: reads.clone(),
        writes: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    });
    let scope = "s".to_owned();
    let target = IdentityTarget::new("model", Dialect::Gemini).unwrap();
    let mut record = IdentityStateRecord::new(IdentityRole::ToolCall, target.clone());
    record.client_call_id = Some("alias".into());
    let version = ready(store.save(&scope, "k", None, &record, None)).unwrap();
    let updated = ready(store.update_late(
        &scope,
        "k",
        version,
        LateIdentityFacts {
            original_call_id: Some("native".into()),
            ..Default::default()
        },
    ))
    .unwrap();
    assert_eq!(updated.record.original_call_id.as_deref(), Some("native"));
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn public_transform_errors_preserve_input_result_and_host_causes() {
    use gproxy_protocol::transform::{DiagnosticKind, Report, TransformError, TransformErrorKind};
    use std::error::Error;
    let result = TransformError::invalid_result("response.output", "missing required tool result");
    assert_eq!(result.kind(), TransformErrorKind::InvalidResult);
    let input = TransformError::shape("request.input", "bad field");
    assert_eq!(input.kind(), TransformErrorKind::InvalidInput);
    let cause = CapabilityError::with_source(
        gproxy_protocol::capability::CapabilityErrorKind::Limit,
        gproxy_protocol::capability::CapabilityErrorStage::BodyTransfer,
        "size",
        std::io::Error::other("original"),
    );
    let err = TransformError::from(cause);
    assert_eq!(err.kind(), TransformErrorKind::Limit);
    let host = err
        .source()
        .unwrap()
        .downcast_ref::<CapabilityError>()
        .unwrap();
    assert!(host.source().unwrap().is::<std::io::Error>());
    let mut report = Report::default();
    report.changed("model", "alias mapped");
    assert_eq!(
        report.diagnostics[0].kind,
        DiagnosticKind::RepresentationChanged
    );
    assert_eq!(report.diagnostics[0].field, "model");
}

#[test]
fn explicit_target_syntax_preserves_valid_ids_and_generates_only_for_invalid_ones() {
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([12; 16]));
    let policy = target()
        .with_syntax(IdSyntax::AsciiIdentifier)
        .with_max_len(64);
    let preserved = flow
        .resolve_or_allocate(
            IdentityRole::ToolCall,
            source("gemini", Some("call-valid_1"), 0),
            &policy,
        )
        .unwrap();
    assert_eq!(preserved.emitted_id, "call-valid_1");
    let generated = flow
        .resolve_or_allocate(
            IdentityRole::ToolCall,
            source("gemini", Some("invalid/id"), u64::MAX),
            &policy,
        )
        .unwrap();
    assert_ne!(generated.emitted_id, "invalid/id");
    assert!(generated.emitted_id.len() <= 64);
    assert!(policy.accepts_source(&generated.emitted_id));
    let policy = TargetIdPolicy::new(Dialect::Claude)
        .with_required_prefix(KnownIdPrefix::Tool)
        .with_generated_prefix(IdentityRole::ToolCall, KnownIdPrefix::Tool);
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([13; 16]));
    let generated = flow
        .resolve_or_allocate(
            IdentityRole::ToolCall,
            source("openai", Some("call_external"), 0),
            &policy,
        )
        .unwrap();
    assert!(generated.emitted_id.starts_with("toolu_"));
}

#[test]
fn deserialized_empty_source_ids_are_missing_not_ambiguous_duplicate_ids() {
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([14; 16]));
    let mut ids = Vec::new();
    for index in [0, 1] {
        let source: SourceIdentity = serde_json::from_value(
            serde_json::json!({"dialect":"gemini","source_id":"","logical_index":index}),
        )
        .unwrap();
        ids.push(
            flow.resolve_or_allocate(IdentityRole::ToolCall, source, &target())
                .unwrap()
                .emitted_id,
        );
    }
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn opaque_field_binding_survives_storage_and_rejects_unbound_legacy_records() {
    let target = IdentityTarget::new("model", Dialect::Claude)
        .unwrap()
        .with_origin("origin")
        .unwrap();
    let mut record = IdentityStateRecord::new(
        IdentityRole::OutputItem(OutputItemKind::Reasoning),
        target.clone(),
    );
    record.opaque_signature = Some(
        OpaqueSignature::new(
            OpaqueField::ClaudeThinkingSignature,
            "same-bytes",
            "origin",
            "model",
        )
        .unwrap(),
    );
    let state = IdentityStateStore::new(FakeStore::default());
    let scope = "conversation".to_owned();
    let version = ready(state.save(&scope, "reasoning", None, &record, None)).unwrap();
    assert_eq!(
        ready(state.read(&scope, "reasoning"))
            .unwrap()
            .record
            .opaque_signature
            .unwrap()
            .field,
        OpaqueField::ClaudeThinkingSignature
    );
    let changed = OpaqueSignature::new(
        OpaqueField::ClaudeRedactedThinkingData,
        "same-bytes",
        "origin",
        "model",
    )
    .unwrap();
    assert!(
        ready(state.update_late(
            &scope,
            "reasoning",
            version,
            LateIdentityFacts {
                opaque_signature: Some(changed),
                ..Default::default()
            }
        ))
        .is_err()
    );
    let mut serialized = serde_json::to_value(&record).unwrap();
    serialized["opaque_signature"]
        .as_object_mut()
        .unwrap()
        .remove("field");
    assert!(serde_json::from_value::<IdentityStateRecord>(serialized).is_err());
    record.opaque_signature.as_mut().unwrap().field = OpaqueField::GeminiPartThoughtSignature;
    record.opaque_signature.as_mut().unwrap().field = OpaqueField::ClaudeThinkingSignature;
    record.schema = 1;
}
