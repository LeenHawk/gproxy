//! The probes: what this deployment can reach, and whether a provider answers.
//!
//! Everything else in this crate answers from the database. These three leave
//! the process, which makes them the only management operations with a cost
//! and a wall clock. Two consequences run through the whole module.
//!
//! A probe that fails to reach anything is still a successful probe. "The
//! upstream is unreachable" is the answer an operator asked for, so a transport
//! failure comes back as `ok: false` with a reason rather than as an `Err`.
//! An `Err` here means the *request* was wrong — an unknown provider, a
//! credential that is not this provider's — which is a different thing and a
//! different fix.
//!
//! [`Connectivity::model_test`] and [`Connectivity::discover_models`] go
//! through `Core` exactly as a caller's request would. They therefore **spend
//! a real credential, consume real upstream quota, settle against any budget
//! the credential is subject to, and write a usage row** attributed to
//! `gproxy-sdk:admin`. There is no dry run; a test that did not really call
//! the upstream would not have tested anything.

use std::{collections::BTreeSet, num::NonZeroU32, sync::Arc, time::Duration};

use futures_util::StreamExt;
use gproxy_channel::channel::{NormalizedUsage, ProviderView};
use gproxy_client::{ConnectionConfig, OutboundClient};
use gproxy_core::{
    CoreData, CredentialData, ExecutionTarget, ProviderData, RequestContext, UsageAttribution,
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest,
    connection::{Bytes, HeaderMap},
    wire::{DeclaredFields, openai::models::Model},
};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::entity::upstream::{model, provider_model};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryTrait, Set, sea_query::OnConflict};
use serde_json::{Value, json};
use web_time::Instant;

use super::{Scope, Writer, catalog, crud};
use crate::{
    SdkError, SdkResult,
    dto::{
        ConnectivityProbeDto, ConnectivityResultDto, ConnectivityScope, ConnectivityTest,
        DiscoveredModelDto, ModelTest, ModelTestResultDto, UsageTokensDto,
    },
    rt,
};

/// Cloudflare reports the address and the edge it saw the request arrive from,
/// which is exactly the question a proxy test asks: not "did a TCP connection
/// open" but "who does the upstream think I am".
const TRACE_V4_URL: &str = "https://1.1.1.1/cdn-cgi/trace";
const TRACE_V6_URL: &str = "https://[2606:4700:4700::1111]/cdn-cgi/trace";
/// A trace body is a few hundred bytes. Anything larger is not a trace.
const TRACE_MAX_BYTES: usize = 8 * 1024;
/// Short on purpose: an operator is watching this run.
const TRACE_TIMEOUT: Duration = Duration::from_secs(10);
/// Long enough for a cold model to answer, short enough to stay a test.
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// The scope a probe runs under. It is not a caller's scope, so a probe never
/// shares credential affinity with real traffic.
const PROBE_SCOPE: &str = "gproxy-sdk:admin";
/// What the usage row a probe writes is attributed to. A person looking at a
/// month of spend should be able to see which of it was operators testing.
const PROBE_KEY: &str = "admin-probe";

pub struct Connectivity<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Connectivity<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Connectivity<'_, C> {
    /// Ask Cloudflare what this deployment looks like from outside, through
    /// the client chain the named scope would use.
    ///
    /// HTTP profiles and proxies resolve independently, exactly as assembly
    /// does: credential, provider, then global. Without a profile, the channel
    /// supplies its HTTP defaults. A draft proxy previews the current scope
    /// without writing it; null resumes inheritance.
    pub async fn test(&self, request: ConnectivityTest) -> SdkResult<ConnectivityResultDto> {
        let (client, source) = self.probe_client(&request).await?;
        let started = Instant::now();
        let (v4, v6) = futures_util::future::join(
            rt::timeout(TRACE_TIMEOUT, trace(client.as_ref(), TRACE_V4_URL, false)),
            rt::timeout(TRACE_TIMEOUT, trace(client.as_ref(), TRACE_V6_URL, true)),
        )
        .await;
        let v4 = v4.unwrap_or_else(|| Err("timeout".into()));
        let v6 = v6.unwrap_or_else(|| Err("timeout".into()));
        let ipv4_error = v4.as_ref().err().cloned();
        let ipv6_error = v6.as_ref().err().cloned();
        let ipv4 = v4.ok();
        let ipv6 = v6.ok();
        let primary = ipv4.as_ref().or(ipv6.as_ref());
        Ok(ConnectivityResultDto {
            ok: primary.is_some(),
            latency_ms: elapsed(started),
            ip: primary.map(|p| p.ip.clone()),
            colo: primary.and_then(|p| p.colo.clone()),
            error: if primary.is_some() {
                None
            } else {
                ipv4_error.clone()
            },
            ipv4,
            ipv6,
            ipv4_error,
            ipv6_error,
            proxy_source: source.into(),
        })
    }

    /// Generate once against one provider and report what came back.
    ///
    /// This spends a credential and writes a usage row; see the module note.
    /// One attempt only: a probe that silently failed over would answer a
    /// question nobody asked.
    pub async fn model_test(&self, request: ModelTest) -> SdkResult<ModelTestResultDto> {
        let snapshot = self.writer.core().snapshot();
        let provider = provider(&snapshot, &request.provider_id)?;
        let credentials = credentials(&snapshot, provider, request.credential_id.as_deref())?;
        // Stream-only HTTP upstreams (such as Codex) still accept a buffered
        // probe through Core's generation conversion and stream collection.
        let dialect = native_dialect(provider, Operation::GenerateContent)
            .or_else(|_| native_dialect(provider, Operation::StreamGenerateContent))?;
        let model = crud::text(&request.model, "model")?;

        let endpoint = gproxy_core::convert::generate_endpoint(dialect, &model, false)
            .map_err(|error| SdkError::invalid(format!("no generation endpoint: {error}")))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        let body = probe_body(dialect, &model);
        let wire = WireRequest {
            method: http::Method::POST,
            path: endpoint.path.clone(),
            query: endpoint.query.clone(),
            headers,
            body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap_or_default())),
        };
        let context = self.context(
            &snapshot,
            OperationKey {
                operation: Operation::GenerateContent,
                dialect,
            },
            provider,
            credentials,
            Some(model.clone()),
        );

        let started = Instant::now();
        let execution = match self.writer.core().generate_content(context, wire).await {
            Ok(execution) => execution,
            Err(error) => {
                return Ok(ModelTestResultDto {
                    latency_ms: elapsed(started),
                    model,
                    error: Some(error.to_string()),
                    ..Default::default()
                });
            }
        };
        let (response, settled) = execution.into_parts();
        let status = response.status;
        // The body has to be read to the end before the funnel can settle, and
        // a failure's body is also the only place the upstream says why.
        let payload = read(response.body, TRACE_MAX_BYTES).await;
        let report = rt::timeout(TRACE_TIMEOUT, settled)
            .await
            .and_then(Result::ok);
        let usage = report.as_ref().and_then(reported);
        let credential_label = report
            .as_ref()
            .and_then(|r| r.exchanges.first())
            .and_then(|e| snapshot.credentials.get(&e.credential_id))
            .map(|c| c.label.clone().unwrap_or_else(|| c.id.clone()));
        let reply = serde_json::from_slice::<Value>(&payload)
            .ok()
            .and_then(|v| {
                v.pointer("/choices/0/message/content")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| {
                        v.get("content").and_then(Value::as_array).map(|a| {
                            a.iter()
                                .filter_map(|x| x.get("text").and_then(Value::as_str))
                                .collect::<Vec<_>>()
                                .join("")
                        })
                    })
                    .or_else(|| {
                        v.pointer("/output/0/content/0/text")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .or_else(|| {
                        v.pointer("/candidates/0/content/parts/0/text")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
            });
        Ok(ModelTestResultDto {
            ok: status.is_success(),
            latency_ms: elapsed(started),
            status: status.as_u16(),
            model,
            usage,
            reply,
            credential_label,
            error: (!status.is_success()).then(|| upstream_error(status, &payload)),
        })
    }

    /// Ask a provider what models it offers, and say which of them this
    /// instance already knows and can already price.
    ///
    /// The directory is requested in the provider's own dialect, so nothing is
    /// converted on the way back and the names are the upstream's own. Like
    /// `model_test` this is a real call on a real credential.
    pub async fn discover_models(
        &self,
        provider_id: &str,
        credential_id: Option<&str>,
    ) -> SdkResult<Vec<DiscoveredModelDto>> {
        self.discover_models_with_credentials(provider_id, credential_id, None)
            .await
    }

    async fn discover_models_with_credentials(
        &self,
        provider_id: &str,
        credential_id: Option<&str>,
        allowed: Option<&BTreeSet<String>>,
    ) -> SdkResult<Vec<DiscoveredModelDto>> {
        let snapshot = self.writer.core().snapshot();
        let provider = provider(&snapshot, provider_id)?;
        let mut credentials = credentials(&snapshot, provider, credential_id)?;
        credentials.retain(|credential| allowed.is_none_or(|ids| ids.contains(&credential.id)));
        if credentials.is_empty() {
            return Err(SdkError::NoTarget(provider_id.into()));
        }
        let dialect = native_dialect(provider, Operation::ListModels)?;
        let path = directory_path(dialect);
        let wire = WireRequest {
            method: http::Method::GET,
            path: path.to_owned(),
            query: None,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::new()),
        };
        // Discovery is an upstream read, independent of downstream list/get
        // routing rules. Keep transport, credentials and endpoint overrides.
        let mut discovery_provider = provider.as_ref().clone();
        discovery_provider
            .operation_rules
            .retain(|rule| rule.operation != Operation::ListModels.id());
        let context = self.context(
            &snapshot,
            OperationKey {
                operation: Operation::ListModels,
                dialect,
            },
            &Arc::new(discovery_provider),
            credentials,
            None,
        );
        let execution = self.writer.core().list_models(context, wire).await?;
        let (response, settled) = execution.into_parts();
        let status = response.status;
        let payload = read(
            response.body,
            snapshot.limits.max_response_body_bytes as usize,
        )
        .await;
        drop(rt::timeout(TRACE_TIMEOUT, settled).await);
        if !status.is_success() {
            return Err(SdkError::Upstream {
                status: status.as_u16(),
                body: upstream_error(status, &payload),
            });
        }
        let document: Value = serde_json::from_slice(&payload).map_err(|error| {
            SdkError::invalid(format!("the model directory is not JSON: {error}"))
        })?;
        let known: BTreeSet<&str> = provider
            .models
            .iter()
            .map(|row| row.upstream_name.as_str())
            .collect();
        let defaults = self
            .writer
            .store()
            .models()
            .query(model::Entity::find())
            .await?;
        model_names(dialect, &document)
            .into_iter()
            .map(|upstream_name| {
                Ok(DiscoveredModelDto {
                    known: known.contains(upstream_name.as_str()),
                    has_default_price: catalog::has_default_price(&upstream_name),
                    metadata: import_metadata(
                        &upstream_name,
                        &defaults,
                        discovered_metadata(dialect, &document, &upstream_name)?,
                    ),
                    upstream_name,
                })
            })
            .collect::<SdkResult<Vec<_>>>()
    }

    /// Refresh the stored catalog using only credentials visible to the caller.
    /// Existing rows, including disabled models and operator metadata, are preserved.
    pub async fn refresh_models(
        &self,
        provider_id: &str,
        allowed_credentials: &BTreeSet<String>,
    ) -> SdkResult<()> {
        let discovered = self
            .discover_models_with_credentials(provider_id, None, Some(allowed_credentials))
            .await?;
        let existing: BTreeSet<String> = self
            .writer
            .store()
            .provider_models()
            .query(
                provider_model::Entity::find()
                    .filter(provider_model::Column::ProviderId.eq(provider_id)),
            )
            .await?
            .into_iter()
            .map(|row| row.upstream_name)
            .collect();
        let statements = discovered
            .into_iter()
            .filter(|row| !existing.contains(&row.upstream_name))
            .map(|row| {
                BatchStatement::Execute(
                    provider_model::Entity::insert(provider_model::ActiveModel {
                        id: Set(crate::ids::random_id()),
                        provider_id: Set(provider_id.into()),
                        upstream_name: Set(row.upstream_name),
                        model_id: Set(None),
                        metadata: Set(row.metadata),
                        enabled: Set(true),
                    })
                    .on_conflict(
                        OnConflict::columns([
                            provider_model::Column::ProviderId,
                            provider_model::Column::UpstreamName,
                        ])
                        .do_nothing_on([provider_model::Column::Id])
                        .to_owned(),
                    )
                    .build(self.writer.backend()),
                )
            })
            .collect::<Vec<_>>();
        if !statements.is_empty() {
            self.writer.commit(statements, &[Scope::Models]).await?;
        }
        Ok(())
    }

    /// Add discovered names to a provider's catalog. Names it already offers
    /// are left alone, so applying a discovery twice is the same as once.
    pub async fn apply_discovered(
        &self,
        provider_id: &str,
        upstream_names: Vec<String>,
    ) -> SdkResult<Vec<String>> {
        let provider_id = crud::text(provider_id, "providerId")?;
        crud::require_rows(
            self.writer.store().providers(),
            "provider",
            std::slice::from_ref(&provider_id),
        )
        .await?;
        let existing: BTreeSet<String> = self
            .writer
            .store()
            .provider_models()
            .query(
                provider_model::Entity::find()
                    .filter(provider_model::Column::ProviderId.eq(&provider_id)),
            )
            .await?
            .into_iter()
            .map(|row| row.upstream_name)
            .collect();

        let defaults = self
            .writer
            .store()
            .models()
            .query(model::Entity::find())
            .await?;
        let mut added = Vec::new();
        let mut statements = Vec::new();
        for name in upstream_names {
            let name = name.trim().to_owned();
            if name.is_empty() || existing.contains(&name) || added.contains(&name) {
                continue;
            }
            statements.push(BatchStatement::Execute(
                self.writer.store().provider_models().insert_statement(
                    provider_model::ActiveModel {
                        id: Set(crate::ids::random_id()),
                        provider_id: Set(provider_id.clone()),
                        upstream_name: Set(name.clone()),
                        model_id: Set(None),
                        metadata: Set(import_metadata(&name, &defaults, json!({}))),
                        enabled: Set(true),
                    },
                )?,
            ));
            added.push(name);
        }
        if !statements.is_empty() {
            self.writer.commit(statements, &[Scope::Models]).await?;
        }
        Ok(added)
    }

    /// The client a probe of this scope should go out through.
    /// Read OpenRouter's public metadata using the instance's configured client.
    /// Importing the returned rows is a separate model write chosen by the caller.
    pub async fn openrouter_models(&self) -> SdkResult<Vec<DiscoveredModelDto>> {
        let (client, _) = self
            .probe_client(&ConnectivityTest {
                scope: ConnectivityScope::Global,
                proxy: None,
            })
            .await?;
        rt::timeout(PROBE_TIMEOUT, async {
            let request = http::Request::get(
                "https://openrouter.ai/api/v1/models?limit=1000&output_modalities=all",
            )
            .body(HttpBody::Bytes(Bytes::new()))
            .map_err(|error| SdkError::invalid(error.to_string()))?;
            let response = client
                .send(request)
                .await
                .map_err(|error| SdkError::invalid(format!("OpenRouter: {error}")))?;
            let status = response.status;
            let body = read(response.body, usize::MAX).await;
            if !status.is_success() {
                return Err(SdkError::invalid(upstream_error(status, &body)));
            }
            let document: Value = serde_json::from_slice(&body)
                .map_err(|error| SdkError::invalid(format!("OpenRouter: {error}")))?;
            let rows = document["data"]
                .as_array()
                .ok_or_else(|| SdkError::invalid("OpenRouter response has no model list"))?;
            let mut seen = BTreeSet::new();
            rows.iter()
                .filter_map(|row| {
                    let id = model_name(Dialect::OpenAi, row)?;
                    let name = id.rsplit('/').next()?.trim();
                    if name.is_empty() || !seen.insert(name.to_ascii_lowercase()) {
                        return None;
                    }
                    Some((name.to_owned(), row))
                })
                .map(|(name, row)| {
                    Ok(DiscoveredModelDto {
                        upstream_name: name,
                        metadata: model_metadata(Some(row))?,
                        known: false,
                        has_default_price: false,
                    })
                })
                .collect::<SdkResult<Vec<_>>>()
        })
        .await
        .ok_or_else(|| SdkError::invalid("OpenRouter model query timed out"))?
    }

    async fn probe_client(
        &self,
        request: &ConnectivityTest,
    ) -> SdkResult<(Arc<dyn OutboundClient>, &'static str)> {
        if let ConnectivityScope::Proxy { url } = &request.scope {
            let value = crud::proxy(Some(json!({"mode":"explicit", "url":url})))?.expect("proxy");
            let proxy =
                serde_json::from_value(value).map_err(|_| SdkError::invalid("invalid proxy"))?;
            return Ok((
                self.client(ConnectionConfig {
                    proxy,
                    ..Default::default()
                })
                .await?,
                "custom",
            ));
        }
        let settings = self.writer.store().settings().get().await?;
        let credential = if let ConnectivityScope::Credential { credential_id } = &request.scope {
            Some(
                self.writer
                    .store()
                    .credentials()
                    .get_many(std::slice::from_ref(credential_id))
                    .await?
                    .into_iter()
                    .next()
                    .flatten()
                    .ok_or_else(|| SdkError::not_found("credential", credential_id))?,
            )
        } else {
            None
        };
        let provider_id = match &request.scope {
            ConnectivityScope::Provider { provider_id } => Some(provider_id.as_str()),
            _ => credential.as_ref().map(|c| c.provider_id.as_str()),
        };
        let provider = if let Some(id) = provider_id {
            Some(
                self.writer
                    .store()
                    .providers()
                    .get_many(&[id.to_owned()])
                    .await?
                    .into_iter()
                    .next()
                    .flatten()
                    .ok_or_else(|| SdkError::not_found("provider", id))?,
            )
        } else {
            None
        };
        let profile = credential
            .as_ref()
            .and_then(|c| c.connection_profile_id.as_deref())
            .or(provider
                .as_ref()
                .and_then(|p| p.connection_profile_id.as_deref()))
            .or(settings
                .as_ref()
                .and_then(|s| s.connection_profile_id.as_deref()));
        let mut config = match profile {
            Some(id) => self.profile_config(id).await?,
            None => provider
                .as_ref()
                .and_then(|p| self.writer.core().channels().get(&p.channel))
                .and_then(|channel| {
                    channel
                        .default_connection_for(gproxy_channel::channel::ConnectionPurpose::Request)
                })
                .unwrap_or_default(),
        };
        let draft = request.proxy.clone().map(crud::proxy).transpose()?;
        let mut credential_proxy = credential.as_ref().and_then(|c| c.proxy.as_ref());
        let mut provider_proxy = provider.as_ref().and_then(|p| p.proxy.as_ref());
        let mut global_proxy = settings.as_ref().and_then(|s| s.proxy.as_ref());
        if let Some(draft) = &draft {
            match request.scope {
                ConnectivityScope::Credential { .. } => credential_proxy = draft.as_ref(),
                ConnectivityScope::Provider { .. } => provider_proxy = draft.as_ref(),
                _ => global_proxy = draft.as_ref(),
            }
        }
        let (proxy, source) =
            gproxy_core::assemble::resolve_proxy(credential_proxy, provider_proxy, global_proxy)
                .map_err(gproxy_core::CoreError::from)?;
        config.proxy = proxy;
        Ok((self.client(config).await?, source))
    }

    async fn profile_config(&self, id: &str) -> SdkResult<ConnectionConfig> {
        let row = self
            .writer
            .store()
            .connection_profiles()
            .get_many(std::slice::from_ref(&id.to_owned()))
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| SdkError::not_found("connection profile", id))?;
        Ok(gproxy_core::assemble::connection_config(&row).map_err(gproxy_core::CoreError::from)?)
    }

    async fn client(&self, config: ConnectionConfig) -> SdkResult<Arc<dyn OutboundClient>> {
        let client = self
            .writer
            .core()
            .clients()
            .get(&config)
            .await
            .map_err(|error| SdkError::invalid(format!("no usable client: {error}")))?;
        Ok(client)
    }

    /// Probes have no budgets or session and a bounded deadline. Discovery
    /// permits a replay after OAuth refresh; generation tests remain one attempt.
    fn context(
        &self,
        snapshot: &Arc<CoreData>,
        operation: OperationKey,
        provider: &Arc<ProviderData>,
        credentials: Vec<Arc<CredentialData>>,
        upstream_model: Option<String>,
    ) -> Arc<RequestContext> {
        let requested_model = upstream_model.clone();
        let upstream_model = upstream_model.map(|name| {
            provider
                .models
                .iter()
                .find(|m| m.enabled && m.variant_names().contains(&name.as_str()))
                .map(|m| m.upstream_name.clone())
                .unwrap_or(name)
        });
        Arc::new(RequestContext {
            request_id: crate::ids::random_id(),
            attribution: UsageAttribution {
                api_key_id: Some(PROBE_KEY.to_owned()),
                model: upstream_model.clone(),
                ..Default::default()
            },
            snapshot: snapshot.clone(),
            scope: PROBE_SCOPE.to_owned(),
            session: None,
            operation,
            target: ExecutionTarget {
                requested_model,
                provider: provider.clone(),
                upstream_model,
                credentials,
            },
            budgets: Vec::new(),
            // A one-attempt request cannot replay after a 401 refresh. Listing
            // models must allow the refreshed credential to fetch the directory.
            max_attempts: if operation.operation == Operation::ListModels {
                NonZeroU32::new(2).expect("nonzero discovery attempts")
            } else {
                NonZeroU32::MIN
            },
            started_at_ms: rt::now_ms(),
            deadline: Some(Instant::now() + PROBE_TIMEOUT),
            cancellation: Default::default(),
        })
    }
}

fn provider<'a>(snapshot: &'a CoreData, id: &str) -> SdkResult<&'a Arc<ProviderData>> {
    snapshot
        .providers
        .get(id)
        .ok_or_else(|| SdkError::not_found("provider", id))
}

/// The credentials a probe may spend: the one that was named, or every one the
/// provider has. An empty set is a configuration problem rather than a probe
/// result, so it is an error.
fn credentials(
    snapshot: &CoreData,
    provider: &Arc<ProviderData>,
    credential_id: Option<&str>,
) -> SdkResult<Vec<Arc<CredentialData>>> {
    let chosen: Vec<Arc<CredentialData>> = provider
        .credential_ids
        .iter()
        .filter(|id| credential_id.is_none_or(|wanted| wanted == id.as_str()))
        .filter_map(|id| snapshot.credentials.get(id).cloned())
        .collect();
    if chosen.is_empty() {
        return Err(match credential_id {
            Some(id) => SdkError::invalid(format!(
                "credential `{id}` does not belong to provider `{}`, or is not loaded",
                provider.entity.id
            )),
            None => SdkError::NoTarget(provider.entity.id.clone()),
        });
    }
    Ok(chosen)
}

/// The dialect this provider speaks natively for `operation`, so the probe is
/// a passthrough and the answer is the upstream's own shape. The websocket
/// envelope variant is skipped: it has no HTTP path.
fn native_dialect(provider: &Arc<ProviderData>, operation: Operation) -> SdkResult<Dialect> {
    provider
        .channel
        .native_dialects(
            ProviderView {
                id: &provider.entity.id,
                channel: &provider.entity.channel,
                base_url: provider.entity.base_url.as_deref(),
                config: &provider.entity.config,
            },
            operation,
        )
        .into_iter()
        .find(|dialect| !matches!(dialect, Dialect::OpenAiResponsesWebSocket))
        .ok_or_else(|| {
            SdkError::invalid(format!(
                "channel `{}` speaks no HTTP dialect for {operation:?}",
                provider.entity.channel
            ))
        })
}

/// The model directory path per family, mirroring core's own
/// `convert::endpoints::list_models_path`, which is not re-exported. The
/// websocket envelope variant never reaches here: `native_dialect` skips it.
fn directory_path(dialect: Dialect) -> String {
    match dialect {
        Dialect::Gemini => "/v1beta/models".to_owned(),
        _ => "/v1/models".to_owned(),
    }
}

/// The smallest generation each family accepts. Small on purpose: this costs
/// the operator money.
fn probe_body(dialect: Dialect, model: &str) -> Value {
    match dialect {
        Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket => json!({
            "model": model,
            "input": "ping",
            "max_output_tokens": 16,
        }),
        Dialect::OpenAiChat => json!({
            "model": model,
            "messages": [{ "role": "user", "content": "ping" }],
            "max_tokens": 16,
        }),
        Dialect::Claude => json!({
            "model": model,
            "messages": [{ "role": "user", "content": "ping" }],
            "max_tokens": 16,
        }),
        Dialect::Gemini => json!({
            "contents": [{ "role": "user", "parts": [{ "text": "ping" }] }],
            "generationConfig": { "maxOutputTokens": 16 },
        }),
    }
}

/// The upstream names in a model directory, per family.
///
/// The protocol crate's wire types are the reference for where a name lives —
/// `data[].id` for OpenAI and Claude, `models[].name` for Gemini — but they
/// are also strict about every other field, and a compatible catalog that
/// omits one still has usable names in it. Discovery reads the names it can
/// see rather than refusing a directory over a field it does not need.
fn model_names(dialect: Dialect, document: &Value) -> Vec<String> {
    let list = if dialect == Dialect::Gemini {
        "models"
    } else {
        "data"
    };
    let mut names: Vec<String> = document
        .get(list)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| model_name(dialect, item).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names.dedup();
    names
}

// Keep directory names and metadata lookup on the same compatibility path.
fn model_name(dialect: Dialect, item: &Value) -> Option<&str> {
    let fields: &[&str] = if dialect == Dialect::Gemini {
        &["name"]
    } else {
        &["id", "model", "name"]
    };
    let name = fields.iter().find_map(|field| item.get(*field)?.as_str())?;
    let name = if dialect == Dialect::Gemini {
        name.strip_prefix("models/").unwrap_or(name)
    } else {
        name
    };
    (!name.is_empty()).then_some(name)
}

/// What the upstream said it spent. A conversion reports the downstream view;
/// a passthrough reports none and its exchanges are the request, which is the
/// same rule the store observer writes a usage row by.
fn reported(report: &gproxy_core::UsageReport) -> Option<UsageTokensDto> {
    if let Some(usage) = &report.downstream_usage {
        return Some(tokens(usage));
    }
    if report.exchanges.is_empty() {
        return None;
    }
    Some(tokens(&NormalizedUsage::aggregate(
        report.exchanges.iter().map(|exchange| &exchange.usage),
    )))
}

fn tokens(usage: &NormalizedUsage) -> UsageTokensDto {
    UsageTokensDto {
        input_tokens: usage.tokens.input_tokens,
        output_tokens: usage.tokens.output_tokens,
        cached_input_tokens: usage.tokens.cached_input_tokens,
        cache_creation_5m_tokens: usage.tokens.cache_creation_5m_tokens,
        cache_creation_30m_tokens: usage.tokens.cache_creation_30m_tokens,
        cache_creation_1h_tokens: usage.tokens.cache_creation_1h_tokens,
        reasoning_tokens: usage.tokens.reasoning_tokens,
    }
}

/// `GET https://www.cloudflare.com/cdn-cgi/trace`, and the `ip=` and `colo=`
/// lines out of the `key=value` body it answers with.
async fn trace(
    client: &dyn OutboundClient,
    url: &str,
    ipv6: bool,
) -> Result<ConnectivityProbeDto, String> {
    let started = Instant::now();
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(url)
        .header(http::header::ACCEPT, "text/plain")
        .body(HttpBody::Bytes(Bytes::new()))
        .map_err(|_| "transport".to_owned())?;
    let response = client
        .send(request)
        .await
        .map_err(|_| "transport".to_owned())?;
    if !response.status.is_success() {
        return Err("http_status".to_owned());
    }
    let body = read(response.body, TRACE_MAX_BYTES).await;
    let text = String::from_utf8_lossy(&body);
    let value = |key: &str| {
        text.lines().find_map(|line| {
            let (name, found) = line.split_once('=')?;
            (name.trim() == key).then(|| found.trim().to_owned())
        })
    };
    let ip = value("ip")
        .and_then(|value| value.parse::<std::net::IpAddr>().ok())
        .filter(|ip| ip.is_ipv6() == ipv6)
        .ok_or_else(|| "invalid_response".to_owned())?;
    Ok(ConnectivityProbeDto {
        ip: ip.to_string(),
        location: value("loc"),
        colo: value("colo"),
        latency_ms: elapsed(started),
    })
}

/// Read a body up to `max_bytes`, keeping whatever arrived. A truncated or
/// failed body is not an error here: the status and the prefix are already
/// enough to report.
async fn read(body: HttpBody, max_bytes: usize) -> Vec<u8> {
    match body {
        HttpBody::Bytes(bytes) => bytes.into_iter().take(max_bytes).collect(),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(Ok(chunk)) = stream.next().await {
                let room = max_bytes.saturating_sub(out.len());
                if room == 0 {
                    break;
                }
                out.extend_from_slice(&chunk[..chunk.len().min(room)]);
            }
            out
        }
    }
}

fn upstream_error(status: http::StatusCode, body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let text = text.trim();
    match text.is_empty() {
        true => format!("upstream answered {}", status.as_u16()),
        false => format!("upstream answered {}: {text}", status.as_u16()),
    }
}

fn elapsed(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn discovered_metadata(dialect: Dialect, document: &Value, name: &str) -> SdkResult<Value> {
    let list = if dialect == Dialect::Gemini {
        "models"
    } else {
        "data"
    };
    let item = document
        .get(list)
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| model_name(dialect, row) == Some(name))
        });
    model_metadata(item)
}

fn model_metadata(item: Option<&Value>) -> SdkResult<Value> {
    let mut fields = item.and_then(Value::as_object).cloned().unwrap_or_default();
    // Directory identity belongs to the provider row, not its metadata.
    fields.insert("id".into(), json!(""));
    fields.insert("object".into(), json!("model"));
    let model: Model = serde_json::from_value(Value::Object(fields))
        .map_err(|error| SdkError::invalid(format!("model metadata: {error}")))?;
    let Value::Object(mut result) = serde_json::to_value(model.into_declared())
        .map_err(|error| SdkError::invalid(format!("model metadata: {error}")))?
    else {
        unreachable!("Model serializes as an object")
    };
    for key in ["id", "slug", "object", "created", "owned_by"] {
        result.remove(key);
    }
    if let Some(item) = item {
        for (key, aliases) in [
            ("display_name", &["display_name", "displayName"][..]),
            ("description", &["description"][..]),
            (
                "context_window",
                &[
                    "context_window",
                    "context_length",
                    "max_input_tokens",
                    "inputTokenLimit",
                ][..],
            ),
            (
                "max_output_tokens",
                &["max_output_tokens", "max_tokens", "outputTokenLimit"][..],
            ),
            (
                "thinking_supported",
                &["thinking_supported", "thinking"][..],
            ),
            ("input_modalities", &["input_modalities"][..]),
            ("output_modalities", &["output_modalities"][..]),
            ("supported_parameters", &["supported_parameters"][..]),
            ("reasoning_levels", &["reasoning_levels"][..]),
            ("service_tiers", &["service_tiers"][..]),
            (
                "generation_methods",
                &["generation_methods", "supportedGenerationMethods"][..],
            ),
        ] {
            if let Some(value) = aliases
                .iter()
                .find_map(|alias| item.get(alias))
                .filter(|v| !v.is_null())
            {
                result.insert(key.into(), value.clone());
            }
        }
    }
    if let Some(item) = item {
        for (key, pointer) in [
            ("input_modalities", "/architecture/input_modalities"),
            ("output_modalities", "/architecture/output_modalities"),
            ("max_output_tokens", "/top_provider/max_completion_tokens"),
        ] {
            if let Some(value) = item.pointer(pointer).filter(|value| !value.is_null()) {
                result.entry(key).or_insert_with(|| value.clone());
            }
        }
        if item.get("id").is_some()
            && let Some(name) = item.get("name").filter(|value| value.is_string())
        {
            result.entry("display_name").or_insert_with(|| name.clone());
        }
    }
    Ok(Value::Object(result))
}

/// Fill upstream omissions, then apply the operator's explicit global overrides.
fn import_metadata(name: &str, defaults: &[model::Model], upstream: Value) -> Value {
    let mut result = catalog::default_metadata(name)
        .as_object()
        .cloned()
        .unwrap_or_default();
    if let Some(upstream) = upstream.as_object() {
        result.extend(
            upstream
                .iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(key, value)| (key.clone(), value.clone())),
        );
    }
    let matched = catalog::matching_model(defaults, name, |row| &row.name);
    if let Some(local) = matched.and_then(|row| row.metadata.as_object()) {
        result.extend(local.clone());
    }
    Value::Object(result)
}
