//! Parity with the v3 operations catalogue: transfer, the static catalogues,
//! the two probes and tokenizer vocabularies.
//!
//! What these pin down is not "the call returns Ok". It is that a
//! configuration survives a move between two instances with *different* master
//! keys, that a secret which cannot be opened is left out rather than written
//! where it would break every later reload, that `Replace` stops at the
//! configuration families, and that a probe reports an unreachable network as
//! a result rather than as a failure of the request.

mod support;

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use base64::Engine;
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture},
    connection::Bytes,
};
use gproxy_sdk::{
    ClientPool, Gproxy, GproxyBuilder, Operator, OutboundClient, SdkError, SyncMode,
    dto::{
        ApplyDefaultPricesRequest, ApplyRulePreset, ConnectivityScope, ConnectivityTest,
        ExportRequest, ImportMode, ImportRequest, ListQuery, ModelTest, ProviderDto, ProviderWrite,
        RuleSetWrite,
    },
    manage::TokenizerFetch,
};
use gproxy_store::entity::identity::user;
use http::{HeaderMap, HeaderValue, StatusCode};
use sea_orm::{DatabaseConnection, Set};
use serde_json::json;

use support::TestChannel;

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::STANDARD;

// ---------------------------------------------------------------------------
// A client that can also fail, which the shared harness's cannot.
// ---------------------------------------------------------------------------

enum Scripted {
    Reply(StatusCode, Vec<(&'static str, &'static str)>, Vec<u8>),
    /// A transport that never produced a response, which is what an
    /// unreachable network looks like from here.
    Fail(&'static str),
}

#[derive(Default)]
struct ProbeClient {
    replies: Mutex<VecDeque<Scripted>>,
    seen: Mutex<Vec<String>>,
}

impl ProbeClient {
    fn script(&self, replies: Vec<Scripted>) {
        *self.replies.lock().unwrap() = replies.into();
    }
    fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

impl OutboundClient for ProbeClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push(format!("{} {}", request.method(), request.uri()));
            match self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted reply")
            {
                Scripted::Fail(reason) => Err(CapabilityError::new(
                    CapabilityErrorKind::Transport,
                    CapabilityErrorStage::Start,
                    reason,
                )),
                Scripted::Reply(status, headers, body) => {
                    let mut map = HeaderMap::new();
                    for (name, value) in headers {
                        map.insert(name, HeaderValue::from_static(value));
                    }
                    Ok(WireResponse {
                        status,
                        headers: map,
                        // A stream, like a real transport: core observes a
                        // response as it is read, and a buffered body would
                        // exercise a path no upstream takes.
                        body: HttpBody::Stream(Box::pin(futures_util::stream::iter([Ok(
                            Bytes::from(body),
                        )]))),
                    })
                }
            }
        })
    }
}

fn json_reply(body: serde_json::Value) -> Scripted {
    Scripted::Reply(
        StatusCode::OK,
        vec![("content-type", "application/json")],
        serde_json::to_vec(&body).unwrap(),
    )
}

fn text_reply(body: &str) -> Scripted {
    Scripted::Reply(
        StatusCode::OK,
        vec![("content-type", "text/plain")],
        body.as_bytes().to_vec(),
    )
}

// ---------------------------------------------------------------------------
// Handles.
// ---------------------------------------------------------------------------

struct Instance {
    gproxy: Gproxy<DatabaseConnection>,
    client: Arc<ProbeClient>,
    /// Kept alive: dropping it removes the storage root under the handle.
    _files: Option<tempfile::TempDir>,
}

impl std::ops::Deref for Instance {
    type Target = Gproxy<DatabaseConnection>;
    fn deref(&self) -> &Self::Target {
        &self.gproxy
    }
}

/// A private in-memory instance sealing with `key`, and optionally with file
/// storage behind it.
async fn instance(key: Option<[u8; 32]>, files: bool) -> Instance {
    let client = Arc::new(ProbeClient::default());
    let mut builder = GproxyBuilder::sqlite_memory()
        .await
        .unwrap()
        .without_default_channels()
        .channel(Arc::new(TestChannel::default()))
        .client_pool(ClientPool::with_client(client.clone()))
        .sync_mode(SyncMode::Manual);
    builder = match key {
        Some(key) => builder.master_key(key),
        None => builder.plaintext_secrets(),
    };
    let directory = files.then(|| tempfile::tempdir().unwrap());
    if let Some(directory) = &directory {
        let operator: Operator =
            gproxy_file::filesystem(directory.path().to_str().unwrap()).unwrap();
        builder = builder.file_storage(operator);
    }
    Instance {
        gproxy: builder.build().await.unwrap(),
        client,
        _files: directory,
    }
}

async fn provider(gproxy: &Gproxy<DatabaseConnection>, name: &str) -> ProviderDto {
    gproxy
        .manage()
        .providers()
        .create(ProviderWrite {
            name: name.to_owned(),
            channel: "test".to_owned(),
            base_url: Some("https://upstream.example".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap()
}

async fn credential(
    gproxy: &Gproxy<DatabaseConnection>,
    provider_id: &str,
    secret: &str,
) -> String {
    gproxy
        .manage()
        .credentials()
        .create(gproxy_sdk::dto::CredentialWrite {
            provider_id: provider_id.to_owned(),
            label: Some("primary".to_owned()),
            auth_kind: "api_key".to_owned(),
            secret: json!({ "api_key": secret }),
            ..Default::default()
        })
        .await
        .unwrap()
        .id
}

// ---------------------------------------------------------------------------
// Transfer.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_export_moves_a_configuration_between_two_master_keys() {
    let source = instance(Some([1u8; 32]), false).await;
    let provider = provider(&source, "upstream").await;
    source
        .manage()
        .providers()
        .update(
            &provider.id,
            gproxy_sdk::dto::ProviderPatch {
                display_name: Some(Some("Friendly provider".into())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let credential_id = credential(&source, &provider.id, "source-key").await;
    source
        .manage()
        .provider_models()
        .create(gproxy_sdk::dto::ProviderModelWrite {
            provider_id: provider.id.clone(),
            upstream_name: "m1".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    let export = source
        .manage()
        .transfer()
        .export(ExportRequest {
            include_secrets: true,
        })
        .await
        .unwrap();
    assert_eq!(export.format_version, 5);
    assert!(!export.secrets_omitted);
    assert_eq!(export.secrets, vec!["aes-gcm".to_owned()]);
    assert_eq!(export.data.providers.len(), 1);
    assert_eq!(export.data.credentials.len(), 1);
    assert!(export.data.credentials[0].secret.is_some());
    // The blob travels sealed: the plaintext is nowhere in the document.
    let document = serde_json::to_string(&export).unwrap();
    assert!(!document.contains("source-key"));

    let destination = instance(Some([2u8; 32]), false).await;
    let report = destination
        .manage()
        .transfer()
        .import(ImportRequest {
            export: export.clone(),
            mode: ImportMode::Merge,
            source_master_key: Some(B64.encode([1u8; 32])),
        })
        .await
        .unwrap();
    assert_eq!(report.credentials_resealed, 1);
    assert_eq!(report.credentials_skipped, 0);
    assert_eq!(report.skipped, 0);
    assert!(report.created >= 3, "{report:?}");

    let landed = destination
        .manage()
        .providers()
        .get(&provider.id)
        .await
        .unwrap();
    assert_eq!(landed.name, "upstream");
    assert_eq!(landed.display_name.as_deref(), Some("Friendly provider"));
    assert_eq!(landed.base_url.as_deref(), Some("https://upstream.example"));
    assert_eq!(
        destination
            .manage()
            .provider_models()
            .list(ListQuery::default())
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    // Resealed under the destination's own key, which is the whole point.
    let revealed = destination
        .manage()
        .credentials()
        .reveal_secret(&credential_id)
        .await
        .unwrap();
    assert_eq!(revealed, json!({ "api_key": "source-key" }));
}

#[tokio::test]
async fn a_wrong_source_key_drops_the_credential_and_keeps_the_rest() {
    let source = instance(Some([1u8; 32]), false).await;
    let provider = provider(&source, "upstream").await;
    let credential_id = credential(&source, &provider.id, "source-key").await;
    let export = source
        .manage()
        .transfer()
        .export(ExportRequest {
            include_secrets: true,
        })
        .await
        .unwrap();

    let destination = instance(Some([2u8; 32]), false).await;
    let report = destination
        .manage()
        .transfer()
        .import(ImportRequest {
            export,
            mode: ImportMode::Merge,
            // Neither the source's key nor the destination's.
            source_master_key: Some(B64.encode([9u8; 32])),
        })
        .await
        .unwrap();
    assert_eq!(report.credentials_skipped, 1);
    assert_eq!(report.skipped, 1);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains(&credential_id)),
        "{:?}",
        report.warnings
    );
    // The provider still landed: one unreadable secret is not a failed import.
    assert!(
        destination
            .manage()
            .providers()
            .get(&provider.id)
            .await
            .is_ok()
    );
    assert!(
        destination
            .manage()
            .credentials()
            .get(&credential_id)
            .await
            .is_err()
    );
    // And the instance is still serviceable, which it would not be if an
    // unopenable secret had been written.
    destination.reload().await.unwrap();
}

#[tokio::test]
async fn replace_prunes_the_configuration_and_leaves_identity_alone() {
    let source = instance(Some([1u8; 32]), false).await;
    let kept = provider(&source, "kept").await;
    let export = source
        .manage()
        .transfer()
        .export(ExportRequest {
            include_secrets: true,
        })
        .await
        .unwrap();

    let destination = instance(Some([1u8; 32]), false).await;
    destination
        .manage()
        .transfer()
        .import(ImportRequest {
            export: export.clone(),
            mode: ImportMode::Merge,
            source_master_key: None,
        })
        .await
        .unwrap();
    let extra = provider(&destination, "local-only").await;
    // An identity row the application layer owns. Nothing in this crate may
    // remove it, whatever mode an import runs in.
    use sea_orm::EntityTrait;
    destination
        .store()
        .users()
        .create_many(vec![user::ActiveModel {
            id: Set("u-1".to_owned()),
            name: Set("root".to_owned()),
            password_hash: Set(None),
            role: Set("admin".to_owned()),
            enabled: Set(true),
            oauth_client_allowlist: Set(None),
            created_at_ms: Set(0),
        }])
        .await
        .unwrap();

    let report = destination
        .manage()
        .transfer()
        .import(ImportRequest {
            export,
            mode: ImportMode::Replace,
            source_master_key: None,
        })
        .await
        .unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.starts_with("replace removed")),
        "{:?}",
        report.warnings
    );
    assert!(
        destination
            .manage()
            .providers()
            .get(&extra.id)
            .await
            .is_err()
    );
    assert!(destination.manage().providers().get(&kept.id).await.is_ok());
    let users = destination
        .store()
        .users()
        .query(user::Entity::find())
        .await
        .unwrap();
    assert_eq!(users.len(), 1, "an import must not touch identity rows");
}

#[tokio::test]
async fn another_format_version_is_refused() {
    let source = instance(None, false).await;
    let mut export = source
        .manage()
        .transfer()
        .export(ExportRequest::default())
        .await
        .unwrap();
    export.format_version = 3;
    let error = source
        .manage()
        .transfer()
        .import(ImportRequest {
            export,
            mode: ImportMode::Merge,
            source_master_key: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(error, SdkError::Invalid(_)), "{error}");
    assert_eq!(error.status_code(), 400);
}

// ---------------------------------------------------------------------------
// Catalogues.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_catalogues_are_readable_and_the_prices_are_idempotent() {
    let gproxy = instance(None, false).await;
    let catalog = gproxy.manage().catalog();

    let channels = catalog.channels();
    assert!(channels.iter().any(|channel| channel.id == "test"));

    let models = catalog.default_models().unwrap();
    assert_eq!(models.models.len(), models.source.total_models);
    assert!(!catalog.tls_presets().is_empty());
    assert!(
        catalog
            .rule_presets()
            .iter()
            .any(|set| set.id == "opencode")
    );

    let request = ApplyDefaultPricesRequest {
        provider_id: None,
        model_ids: vec!["anthropic/claude-sonnet-4".to_owned()],
        overwrite: false,
    };
    let first = catalog.apply_default_prices(request.clone()).await.unwrap();
    assert_eq!(first.created, 1);
    assert_eq!(first.unmatched, 0);

    let rules = gproxy
        .manage()
        .pricing()
        .rules()
        .list(ListQuery::default())
        .await
        .unwrap();
    assert_eq!(rules.items.len(), 1);
    let rule = &rules.items[0];
    assert!(rule.model_pattern.contains("claude-sonnet-4"));
    assert_eq!(rule.currency, "USD");
    let rates = gproxy
        .manage()
        .pricing()
        .rates()
        .list(ListQuery {
            price_rule_id: Some(rule.id.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(rates.items.iter().any(|rate| rate.metric == "input_tokens"));
    let tiers = gproxy
        .manage()
        .pricing()
        .tiers()
        .list(ListQuery {
            price_rule_id: Some(rule.id.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        !tiers.items.is_empty(),
        "this model has a long-context tier"
    );

    // Re-applying leaves the operator's copy alone.
    let again = catalog.apply_default_prices(request).await.unwrap();
    assert_eq!(again.created, 0);
    assert_eq!(again.skipped, 1);
    assert_eq!(
        gproxy
            .manage()
            .pricing()
            .rules()
            .list(ListQuery::default())
            .await
            .unwrap()
            .items
            .len(),
        1
    );

    let unknown = catalog
        .apply_default_prices(ApplyDefaultPricesRequest {
            provider_id: None,
            model_ids: vec!["nobody/nothing".to_owned()],
            overwrite: false,
        })
        .await
        .unwrap();
    assert_eq!(unknown.unmatched, 1);
}

#[tokio::test]
async fn default_prices_match_qualified_models_without_renaming_provider_rules() {
    let gproxy = instance(None, false).await;
    let provider = provider(&gproxy, "prices").await;
    let names = [
        "anthropic/claude-sonnet-4.5-20250929",
        "OPENAI/GPT-5.6-SOL-PRO:BATCH",
    ];
    let catalog = gproxy.manage().catalog();
    let applied = catalog
        .apply_default_prices(ApplyDefaultPricesRequest {
            provider_id: Some(provider.id.clone()),
            model_ids: names.iter().map(|name| (*name).to_owned()).collect(),
            overwrite: false,
        })
        .await
        .unwrap();
    assert_eq!(applied.created, 2);
    assert_eq!(applied.unmatched, 0);
    let rules = gproxy
        .manage()
        .pricing()
        .rules()
        .list(ListQuery {
            provider_id: Some(provider.id.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(rules.items.len(), 2);
    for name in names {
        assert!(rules.items.iter().any(|rule| rule.model_pattern == name));
    }
    let again = catalog
        .apply_default_prices(ApplyDefaultPricesRequest {
            provider_id: Some(provider.id),
            model_ids: names.iter().map(|name| (*name).to_owned()).collect(),
            overwrite: false,
        })
        .await
        .unwrap();
    assert_eq!(again.created, 0);
    assert_eq!(again.skipped, 2);
    let global = catalog
        .apply_default_prices(ApplyDefaultPricesRequest {
            provider_id: None,
            model_ids: vec![
                "claude-sonnet-4".to_owned(),
                "anthropic/claude-sonnet-4".to_owned(),
            ],
            overwrite: false,
        })
        .await
        .unwrap();
    assert_eq!(global.created, 1);
    assert_eq!(global.skipped, 1);
}

#[tokio::test]
async fn catalog_audio_tiers_keep_their_units_when_applied() {
    let gproxy = instance(None, false).await;
    gproxy
        .manage()
        .catalog()
        .apply_default_prices(ApplyDefaultPricesRequest {
            provider_id: None,
            model_ids: vec!["google/gemini-2.5-pro".to_owned()],
            overwrite: false,
        })
        .await
        .unwrap();
    let rules = gproxy
        .manage()
        .pricing()
        .rules()
        .list(ListQuery::default())
        .await
        .unwrap();
    let tiers = gproxy
        .manage()
        .pricing()
        .tiers()
        .list(ListQuery {
            price_rule_id: Some(rules.items[0].id.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    let tier = &tiers.items[0];
    assert_eq!(tier.min_prompt_tokens, 200000);
    assert_eq!(tier.audio_input_per_million.as_deref(), Some("2.5"));
    assert_eq!(tier.cached_audio_input_per_million.as_deref(), Some("0.25"));
}

#[tokio::test]
async fn a_rule_preset_becomes_rewrite_rules() {
    let gproxy = instance(None, false).await;
    let set = gproxy
        .manage()
        .rewrite()
        .sets()
        .create(RuleSetWrite {
            name: "clients".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let rules = gproxy
        .manage()
        .catalog()
        .apply_rule_preset(ApplyRulePreset {
            rule_set_id: set.id.clone(),
            preset_id: "opencode".to_owned(),
        })
        .await
        .unwrap();
    // Every rule compiled through core's own compiler on the way in.
    assert!(rules.len() > 10, "{}", rules.len());
    assert!(rules.iter().all(|rule| rule.rule_set_id == set.id));
    assert!(rules.iter().any(|rule| rule.phase == "response"));
    assert!(
        rules
            .iter()
            .all(|rule| rule.filter_header_pattern.as_deref() == Some("^user-agent: opencode/"))
    );
}

// ---------------------------------------------------------------------------
// Probes.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_trace_answers_with_the_egress_address() {
    let gproxy = instance(None, false).await;
    gproxy.client.script(vec![
        text_reply("fl=1f2\nh=1.1.1.1\nip=203.0.113.9\nts=1\ncolo=NRT\n"),
        text_reply("fl=1f2\nh=2606:4700:4700::1111\nip=2001:db8::9\nts=1\ncolo=NRT\n"),
    ]);
    let result = gproxy
        .manage()
        .connectivity()
        .test(ConnectivityTest {
            proxy: None,
            scope: ConnectivityScope::Global,
        })
        .await
        .unwrap();
    assert!(result.ok, "{result:?}");
    assert_eq!(result.ip.as_deref(), Some("203.0.113.9"));
    assert_eq!(result.colo.as_deref(), Some("NRT"));
    assert_eq!(
        result.ipv4.as_ref().map(|probe| probe.ip.as_str()),
        Some("203.0.113.9")
    );
    assert_eq!(
        result.ipv6.as_ref().map(|probe| probe.ip.as_str()),
        Some("2001:db8::9")
    );
    assert_eq!(gproxy.client.seen().len(), 2);
    assert!(result.error.is_none());
    assert!(
        gproxy.client.seen()[0].contains("cdn-cgi/trace"),
        "{:?}",
        gproxy.client.seen()
    );
}

#[tokio::test]
async fn an_unreachable_network_is_a_result_not_an_error() {
    let gproxy = instance(None, false).await;
    gproxy.client.script(vec![
        Scripted::Fail("IPv4 connection refused"),
        Scripted::Fail("IPv6 connection refused"),
    ]);
    let result = gproxy
        .manage()
        .connectivity()
        .test(ConnectivityTest {
            proxy: None,
            scope: ConnectivityScope::Proxy {
                url: "http://127.0.0.1:9".to_owned(),
            },
        })
        .await
        .expect("a failed probe is still a probe");
    assert!(!result.ok);
    assert!(result.ip.is_none());
    assert!(result.error.is_some(), "{result:?}");
    assert!(result.ipv4_error.is_some());
    assert!(result.ipv6_error.is_some());
}

#[tokio::test]
async fn a_model_test_reaches_the_upstream_and_reports_its_answer() {
    let gproxy = instance(None, false).await;
    let provider = provider(&gproxy, "upstream").await;
    credential(&gproxy, &provider.id, "live-key").await;
    gproxy.client.script(vec![json_reply(json!({
        "id": "resp_1",
        "object": "response",
        "output": [],
        "usage": { "input_tokens": 7, "output_tokens": 3 },
    }))]);

    let result = gproxy
        .manage()
        .connectivity()
        .model_test(ModelTest {
            provider_id: provider.id.clone(),
            model: "m1".to_owned(),
            credential_id: None,
        })
        .await
        .unwrap();
    assert!(result.ok, "{result:?}");
    assert_eq!(result.status, 200);
    assert_eq!(result.model, "m1");
    // The probe settles like a real call: whatever the deployment would have
    // metered for it — here the local estimate, since the scripted channel
    // extracts no upstream usage — comes back with the result.
    let usage = result.usage.expect("a settled probe reports what it spent");
    assert!(usage.input_tokens.is_some(), "{usage:?}");
    assert!(
        gproxy.client.seen()[0].contains("https://upstream.example/v1/responses"),
        "{:?}",
        gproxy.client.seen()
    );
}

#[tokio::test]
async fn discovery_reads_both_directory_shapes() {
    let gproxy = instance(None, false).await;
    let openai = provider(&gproxy, "openai-like").await;
    credential(&gproxy, &openai.id, "k1").await;
    // A provider whose channel speaks only Claude, so the directory comes back
    // in Claude's own shape rather than OpenAI's.
    let claude = gproxy
        .manage()
        .providers()
        .create(ProviderWrite {
            name: "claude-like".to_owned(),
            channel: "test".to_owned(),
            base_url: Some("https://claude.example".to_owned()),
            config: Some(json!({ "dialects": ["claude"] })),
            ..Default::default()
        })
        .await
        .unwrap();
    credential(&gproxy, &claude.id, "k2").await;

    gproxy.client.script(vec![json_reply(json!({
        "object": "list",
        "data": [
            { "id": "gpt-5.6-sol", "object": "model" },
            { "id": "anthropic/claude-sonnet-4", "object": "model" },
        ],
    }))]);
    let found = gproxy
        .manage()
        .connectivity()
        .discover_models(&openai.id, None)
        .await
        .unwrap();
    let names: Vec<&str> = found
        .iter()
        .map(|model| model.upstream_name.as_str())
        .collect();
    assert_eq!(names, vec!["anthropic/claude-sonnet-4", "gpt-5.6-sol"]);
    assert!(found.iter().all(|model| !model.known));
    assert!(
        found
            .iter()
            .any(|model| model.upstream_name == "anthropic/claude-sonnet-4"
                && model.has_default_price)
    );

    gproxy.client.script(vec![json_reply(json!({
        "data": [
            { "id": "claude-sonnet-4-5", "type": "model", "display_name": "Claude Sonnet 4.5" },
        ],
        "first_id": "claude-sonnet-4-5",
        "last_id": "claude-sonnet-4-5",
        "has_more": false,
    }))]);
    let found = gproxy
        .manage()
        .connectivity()
        .discover_models(&claude.id, None)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].upstream_name, "claude-sonnet-4-5");

    // Applying what was found is idempotent, and a second discovery says so.
    let added = gproxy
        .manage()
        .connectivity()
        .apply_discovered(&claude.id, vec!["claude-sonnet-4-5".to_owned()])
        .await
        .unwrap();
    assert_eq!(added, vec!["claude-sonnet-4-5".to_owned()]);
    let again = gproxy
        .manage()
        .connectivity()
        .apply_discovered(&claude.id, vec!["claude-sonnet-4-5".to_owned()])
        .await
        .unwrap();
    assert!(again.is_empty());
}

#[tokio::test]
async fn discovery_applies_catalog_and_local_overrides_without_replacing_provider_edits() {
    let gproxy = instance(None, false).await;
    let provider = provider(&gproxy, "metadata").await;
    credential(&gproxy, &provider.id, "k1").await;
    gproxy
        .manage()
        .models()
        .create(gproxy_sdk::dto::ModelWrite {
            name: "anthropic/claude-sonnet-4".to_owned(),
            metadata: Some(json!({"context_window": 123456, "display_name": "Local Sonnet"})),
            ..Default::default()
        })
        .await
        .unwrap();
    gproxy.client.script(vec![json_reply(json!({"data": [
        {"id": "claude-sonnet-4", "context_window": 999, "max_output_tokens": 1234},
        {"id": "unknown-model"},
        {"id": "upstream-only", "name": "Upstream only", "context_length": 4096,
         "architecture": {"input_modalities": ["audio"], "output_modalities": ["text"]},
         "top_provider": {"max_completion_tokens": 256}}
    ]}))]);
    let found = gproxy
        .manage()
        .connectivity()
        .discover_models(&provider.id, None)
        .await
        .unwrap();
    let upstream = found
        .iter()
        .find(|row| row.upstream_name == "upstream-only")
        .unwrap();
    assert_eq!(upstream.metadata["context_window"], 4096);
    assert_eq!(upstream.metadata["max_output_tokens"], 256);
    assert_eq!(upstream.metadata["display_name"], "Upstream only");
    assert_eq!(upstream.metadata["input_modalities"], json!(["audio"]));
    let sonnet = found
        .iter()
        .find(|row| row.upstream_name == "claude-sonnet-4")
        .unwrap();
    assert_eq!(sonnet.metadata["context_window"], 123456);
    assert_eq!(sonnet.metadata["max_output_tokens"], 1234);
    assert_eq!(sonnet.metadata["display_name"], "Local Sonnet");
    assert!(
        sonnet.metadata["input_modalities"]
            .as_array()
            .unwrap()
            .contains(&json!("image"))
    );
    assert_eq!(
        found
            .iter()
            .find(|row| row.upstream_name == "unknown-model")
            .unwrap()
            .metadata,
        json!({})
    );
    gproxy
        .manage()
        .connectivity()
        .apply_discovered(&provider.id, vec!["claude-sonnet-4".to_owned()])
        .await
        .unwrap();
    let rows = gproxy
        .manage()
        .provider_models()
        .list(ListQuery {
            provider_id: Some(provider.id.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    let row = &rows.items[0];
    assert_eq!(row.metadata["context_window"], 123456);
    gproxy
        .manage()
        .provider_models()
        .update(
            &row.id,
            gproxy_sdk::dto::ProviderModelPatch {
                metadata: Some(json!({"context_window": 77})),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    gproxy
        .manage()
        .connectivity()
        .apply_discovered(&provider.id, vec!["claude-sonnet-4".to_owned()])
        .await
        .unwrap();
    assert_eq!(
        gproxy
            .manage()
            .provider_models()
            .get(&row.id)
            .await
            .unwrap()
            .metadata["context_window"],
        77
    );
}

// ---------------------------------------------------------------------------
// Tokenizer vocabularies.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_source_token_round_trips_sealed() {
    let gproxy = instance(Some([7u8; 32]), false).await;
    let tokenizer = gproxy.manage().tokenizer();
    assert!(!tokenizer.auth().await.unwrap().configured);

    assert!(
        tokenizer
            .set_auth(Some("hf_secret_value".to_owned()))
            .await
            .unwrap()
            .configured
    );
    assert_eq!(tokenizer.reveal_auth().await.unwrap(), "hf_secret_value");

    // Sealed at rest: the token is not in the column.
    let stored = gproxy
        .store()
        .settings()
        .get()
        .await
        .unwrap()
        .unwrap()
        .tokenizer_auth_token
        .unwrap();
    assert!(
        !stored
            .windows("hf_secret_value".len())
            .any(|window| window == b"hf_secret_value")
    );

    assert!(!tokenizer.set_auth(None).await.unwrap().configured);
    assert!(tokenizer.reveal_auth().await.is_err());
}

#[tokio::test]
async fn a_fetched_vocabulary_is_stored_and_selected() {
    let gproxy = instance(None, true).await;
    gproxy
        .manage()
        .settings()
        .update(gproxy_sdk::dto::SettingsPatch {
            instance: Some(gproxy_sdk::dto::InstanceSettingsPatch {
                enable_tokenizer_download: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    let model = gproxy
        .manage()
        .models()
        .create(gproxy_sdk::dto::ModelWrite {
            name: "sonnet".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    gproxy
        .client
        .script(vec![json_reply(json!({ "model": { "vocab": {} } }))]);

    let vocabulary = gproxy
        .manage()
        .tokenizer()
        .fetch(TokenizerFetch {
            repo: "anthropic/tokenizer".to_owned(),
            filename: None,
            model_id: Some(model.id.clone()),
            set_as_default: true,
        })
        .await
        .unwrap();
    assert!(vocabulary.size_bytes > 0);
    assert_eq!(vocabulary.filename.as_deref(), Some("tokenizer.json"));
    assert!(
        gproxy.client.seen()[0]
            .contains("https://huggingface.co/anthropic/tokenizer/resolve/main/tokenizer.json"),
        "{:?}",
        gproxy.client.seen()
    );

    let listed = gproxy.manage().tokenizer().vocabularies().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].file_id, vocabulary.file_id);
    assert_eq!(listed[0].models, vec![model.id.clone()]);
    assert!(listed[0].is_default);
    assert_eq!(
        gproxy
            .manage()
            .models()
            .get(&model.id)
            .await
            .unwrap()
            .vocabulary_file_id
            .as_deref(),
        Some(vocabulary.file_id.as_str())
    );
    // Nothing is in flight any more.
    assert!(gproxy.manage().tokenizer().progress().is_none());

    gproxy
        .manage()
        .tokenizer()
        .delete(&vocabulary.file_id)
        .await
        .unwrap();
    assert!(
        gproxy
            .manage()
            .tokenizer()
            .vocabularies()
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_missing_vocabulary_keeps_the_upstream_status() {
    let gproxy = instance(None, true).await;
    gproxy
        .manage()
        .settings()
        .update(gproxy_sdk::dto::SettingsPatch {
            instance: Some(gproxy_sdk::dto::InstanceSettingsPatch {
                enable_tokenizer_download: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    gproxy.client.script(vec![Scripted::Reply(
        StatusCode::NOT_FOUND,
        Vec::new(),
        b"no such file".to_vec(),
    )]);
    let error = gproxy
        .manage()
        .tokenizer()
        .fetch(TokenizerFetch {
            repo: "anthropic/tokenizer".to_owned(),
            filename: None,
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(
        matches!(error, SdkError::Upstream { status: 404, .. }),
        "{error}"
    );
    assert_eq!(error.status_code(), 404);
    assert!(
        gproxy
            .manage()
            .tokenizer()
            .vocabularies()
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn without_file_storage_a_vocabulary_has_nowhere_to_go() {
    let gproxy = instance(None, false).await;
    gproxy
        .manage()
        .settings()
        .update(gproxy_sdk::dto::SettingsPatch {
            instance: Some(gproxy_sdk::dto::InstanceSettingsPatch {
                enable_tokenizer_download: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    let error = gproxy
        .manage()
        .tokenizer()
        .fetch(TokenizerFetch {
            repo: "anthropic/tokenizer".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(matches!(error, SdkError::Unsupported(_)), "{error}");
    assert_eq!(error.status_code(), 501);
}

#[tokio::test]
async fn disabling_vocabulary_download_prevents_any_upstream_request() {
    let gproxy = instance(None, true).await;
    let error = gproxy
        .manage()
        .tokenizer()
        .fetch(TokenizerFetch {
            repo: "test/tokenizer".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("disabled"));
    assert!(gproxy.client.seen().is_empty());
}
