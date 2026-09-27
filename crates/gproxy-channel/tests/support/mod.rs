//! A scripted `ServiceCaller` for the vendor service tests: in-memory
//! bindings, a fixed identity and usage, a configurable role.
#![allow(dead_code)]

use gproxy_channel::channel::{
    CallerIdentity, CallerRole, CallerUsage, OperationFuture, ResourceBindingRecord, ServiceCaller,
};
use std::sync::Mutex;

pub struct ScriptCaller {
    pub role: CallerRole,
    pub identity: CallerIdentity,
    pub usage: CallerUsage,
    pub bindings: Mutex<Vec<ResourceBindingRecord>>,
}

impl ScriptCaller {
    pub fn new(role: CallerRole, id: &str) -> Self {
        Self {
            role,
            identity: CallerIdentity {
                id: id.to_owned(),
                display_name: None,
            },
            usage: CallerUsage::default(),
            bindings: Mutex::new(Vec::new()),
        }
    }
    pub fn member(id: &str) -> Self {
        Self::new(CallerRole::Member, id)
    }
    pub fn admin(id: &str) -> Self {
        Self::new(CallerRole::Admin, id)
    }
    pub fn with_usage(mut self, usage: CallerUsage) -> Self {
        self.usage = usage;
        self
    }
    pub fn with_binding(
        self,
        kind: &str,
        upstream_id: &str,
        credential_id: &str,
        summary: serde_json::Value,
    ) -> Self {
        self.bindings.lock().unwrap().push(ResourceBindingRecord {
            kind: kind.to_owned(),
            upstream_id: upstream_id.to_owned(),
            credential_id: credential_id.to_owned(),
            summary,
        });
        self
    }
    pub fn bound(&self) -> Vec<ResourceBindingRecord> {
        self.bindings.lock().unwrap().clone()
    }
}

impl ServiceCaller for ScriptCaller {
    fn role(&self) -> CallerRole {
        self.role
    }
    fn identity(&self) -> &CallerIdentity {
        &self.identity
    }
    fn usage<'a>(&'a self) -> OperationFuture<'a, CallerUsage> {
        let usage = self.usage.clone();
        Box::pin(async move { Ok(usage) })
    }
    fn find_binding<'a>(
        &'a self,
        kind: &'a str,
        upstream_id: &'a str,
    ) -> OperationFuture<'a, Option<ResourceBindingRecord>> {
        let found = self
            .bindings
            .lock()
            .unwrap()
            .iter()
            .find(|b| b.kind == kind && b.upstream_id == upstream_id)
            .cloned();
        Box::pin(async move { Ok(found) })
    }
    fn list_bindings<'a>(
        &'a self,
        kind: &'a str,
    ) -> OperationFuture<'a, Vec<ResourceBindingRecord>> {
        let found = self
            .bindings
            .lock()
            .unwrap()
            .iter()
            .filter(|b| b.kind == kind)
            .cloned()
            .collect();
        Box::pin(async move { Ok(found) })
    }
    fn save_binding<'a>(&'a self, record: ResourceBindingRecord) -> OperationFuture<'a, ()> {
        self.bindings.lock().unwrap().push(record);
        Box::pin(async { Ok(()) })
    }
    fn delete_binding<'a>(
        &'a self,
        kind: &'a str,
        upstream_id: &'a str,
    ) -> OperationFuture<'a, ()> {
        self.bindings
            .lock()
            .unwrap()
            .retain(|b| !(b.kind == kind && b.upstream_id == upstream_id));
        Box::pin(async { Ok(()) })
    }
}

// --------------------------------------------- fixtures for the API-key fleet

use gproxy_channel::OutboundClient;
use gproxy_channel::channel::{
    ChannelError, CredentialContext, CredentialView, ProviderView, QuotaQuery, QuotaSnapshot,
};
use gproxy_protocol::capability::{CapabilityError, CapabilityFuture};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{HttpBody, WireRequest, WireResponse};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::Value;

pub fn provider<'a>(
    channel: &'a str,
    config: &'a Value,
    base_url: Option<&'a str>,
) -> ProviderView<'a> {
    ProviderView {
        id: "p",
        channel,
        base_url,
        config,
    }
}

pub fn credential<'a>(
    auth_kind: &'a str,
    secret: &'a Value,
    metadata: &'a Value,
) -> CredentialView<'a> {
    CredentialView {
        id: "c",
        provider_id: "p",
        auth_kind,
        secret,
        metadata,
        version: 1,
        expires_at_ms: None,
    }
}

/// A caller's request, carrying the source authentication and hop-by-hop
/// headers every channel is expected to drop.
pub fn request(path: &str, query: Option<&str>) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer client-secret"),
    );
    headers.insert("x-api-key", HeaderValue::from_static("client-secret"));
    headers.insert("host", HeaderValue::from_static("gproxy.local"));
    headers.insert("content-length", HeaderValue::from_static("2"));
    headers.insert("anthropic-beta", HeaderValue::from_static("files-api"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: query.map(str::to_owned),
        headers,
        body: HttpBody::Bytes(Bytes::from_static(b"{}")),
    }
}

/// One scripted reply, recording what was asked for.
pub struct OneShot {
    pub status: StatusCode,
    pub body: String,
    pub seen: Mutex<Vec<(String, HeaderMap, Vec<u8>)>>,
}

impl OneShot {
    pub fn new(status: StatusCode, body: String) -> Self {
        Self {
            status,
            body,
            seen: Mutex::new(Vec::new()),
        }
    }

    /// The URL, headers and body of the call at `index`.
    pub fn call(&self, index: usize) -> (String, HeaderMap, Vec<u8>) {
        self.seen.lock().unwrap()[index].clone()
    }
}

impl OutboundClient for OneShot {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let bytes = match body {
                HttpBody::Bytes(bytes) => bytes.to_vec(),
                HttpBody::Stream(_) => Vec::new(),
            };
            self.seen
                .lock()
                .unwrap()
                .push((parts.uri.to_string(), parts.headers, bytes));
            Ok(WireResponse {
                status: self.status,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from(self.body.clone())),
            })
        })
    }
}

/// Run a channel's quota probe against one scripted reply and return the URL
/// it asked for alongside the snapshot.
pub async fn quota<C: QuotaQuery>(
    channel: &C,
    channel_id: &str,
    secret: &Value,
    base_url: Option<&str>,
    status: StatusCode,
    body: String,
) -> Result<(String, QuotaSnapshot), ChannelError> {
    let config = Value::Object(Default::default());
    let client = OneShot::new(status, body);
    let snapshot = channel
        .query(CredentialContext {
            provider: provider(channel_id, &config, base_url),
            credential: credential("api_key", secret, &Value::Null),
            client: &client,
        })
        .await?;
    let url = client.call(0).0;
    Ok((url, snapshot))
}

// ------------------------------------------------------- quota contract

use gproxy_channel::channel::{QuotaDimension, QuotaEntry, QuotaModel, QuotaScope, classify_by_id};

/// Response headers captured as `name: value` lines, as in `fixtures/quota`.
pub fn header_fixture(text: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (name, value) = line.split_once(':').expect("name: value");
        headers.append(
            http::header::HeaderName::from_bytes(name.trim().as_bytes()).unwrap(),
            HeaderValue::from_str(value.trim()).unwrap(),
        );
    }
    headers
}

/// The channel quota contract over real replies: every entry lands on a
/// declared dimension under that dimension's id and scope, or is listed as
/// observe-only (a trailing `*` matches a prefix). A new upstream window
/// fails here instead of silently accruing no cost.
pub fn assert_quota_contract(
    model: Option<&dyn QuotaModel>,
    declared: &[QuotaDimension],
    entries: &[QuotaEntry],
    observe_only: &[&str],
) {
    let listed = |id: &str| {
        observe_only
            .iter()
            .any(|pattern| match pattern.strip_suffix('*') {
                Some(prefix) => id.starts_with(prefix),
                None => id == *pattern,
            })
    };
    for dimension in declared {
        assert!(
            !listed(&dimension.id),
            "declared `{}` is also listed as observe-only",
            dimension.id
        );
    }
    for entry in entries {
        let placed = match model {
            Some(model) => model.classify(declared, entry),
            None => classify_by_id(declared, entry),
        };
        match placed {
            Some(dimension) => {
                assert_eq!(entry.id, dimension.id, "entry id is the dimension id");
                assert_eq!(
                    entry.source_id, dimension.id,
                    "source id is the dimension id"
                );
                // An `Unknown` declaration defers to the entry's own scope.
                if dimension.scope != QuotaScope::Unknown {
                    assert_eq!(
                        entry.model_scope, dimension.scope,
                        "`{}` scope agrees with its declaration",
                        entry.id
                    );
                }
            }
            None => assert!(
                listed(&entry.id),
                "`{}` is neither declared nor listed as observe-only",
                entry.id
            ),
        }
    }
}

/// What the host settles a complete reply with, once the channel has
/// returned it: the standard reading of the operation, then the channel's
/// extras over the body.
pub fn settled(
    channel: &dyn gproxy_channel::BaseChannel,
    operation: gproxy_protocol::Operation,
    dialect: gproxy_protocol::Dialect,
    headers: &http::HeaderMap,
    body: &[u8],
) -> Option<gproxy_channel::channel::NormalizedUsage> {
    let root = serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);
    gproxy_channel::channel::with_extras(
        channel.usage_extras(),
        gproxy_channel::channel::UsageSource {
            operation: gproxy_protocol::OperationKey { operation, dialect },
            headers,
            root: &root,
        },
        gproxy_protocol::usage::whole(operation, dialect, body),
    )
}

/// What the host settles a stream with, once the channel has returned it
/// and the stream ended on its own: the standard reading of the operation,
/// then the channel's extras over the last event that carried usage.
pub fn settled_stream(
    channel: &dyn gproxy_channel::BaseChannel,
    operation: gproxy_protocol::Operation,
    dialect: gproxy_protocol::Dialect,
    headers: &http::HeaderMap,
    wire: &[u8],
) -> Option<gproxy_channel::channel::NormalizedUsage> {
    use gproxy_protocol::connection::StreamFraming;
    use gproxy_protocol::usage::{UsageReader, UsageStreamEnd, UsageTransport};
    // Framed as the response declares; otherwise the way the dialect
    // streams by default, as the host reads it.
    let framing = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.starts_with("text/event-stream"))
        .map(|_| StreamFraming::Sse);
    let mut reader = UsageReader::new(operation, dialect, UsageTransport::Http { framing })
        .expect("a watchable stream");
    reader.keep_usage_event();
    for chunk in wire.chunks(7) {
        reader.push(chunk);
    }
    let root = reader.usage_event().unwrap_or(serde_json::Value::Null);
    gproxy_channel::channel::with_extras(
        channel.usage_extras(),
        gproxy_channel::channel::UsageSource {
            operation: gproxy_protocol::OperationKey { operation, dialect },
            headers,
            root: &root,
        },
        reader.finish(UsageStreamEnd::Complete),
    )
}
