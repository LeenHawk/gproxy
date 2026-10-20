//! Realtime route dispatch and scope-isolated call-to-credential continuations.
pub mod request;
use crate::{Core, CoreError, CoreResult, RequestContext, keys};
pub use gproxy_protocol::wire::openai::realtime::RealtimeRoute;
use gproxy_protocol::{
    HttpBody, Operation, WireRequest, WireResponse,
    codec::{CodecLimits, read_http_body},
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

const CALL_TTL: Duration = Duration::from_secs(24 * 60 * 60);
#[derive(Serialize, Deserialize)]
struct CallBinding {
    credential_id: String,
    model: Option<String>,
    #[serde(default)]
    requested_model: Option<String>,
    #[serde(default)]
    live: bool,
}

pub(crate) async fn remember_live_socket(
    cache: &dyn gproxy_cache::Cache,
    request: &RequestContext,
    credential: &str,
    frame: &gproxy_protocol::connection::WsFrame,
) -> CoreResult<()> {
    if request.operation.operation != Operation::ConnectLive {
        return Ok(());
    }
    let gproxy_protocol::connection::WsFrame::Text(text) = frame else {
        return Ok(());
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Ok(());
    };
    if value["type"] != "session.started" {
        return Ok(());
    }
    let id = value
        .pointer("/session/id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.is_empty() && !id.contains('/') && !matches!(*id, "." | ".."))
        .ok_or_else(|| {
            CoreError::InvalidTarget("Live session.started omitted session.id".into())
        })?;
    let binding = CallBinding {
        credential_id: credential.to_owned(),
        model: request.target.upstream_model.clone(),
        requested_model: request.attribution.model.clone(),
        live: true,
    };
    let bytes = serde_json::to_vec(&binding)
        .map_err(|error| CoreError::InvalidTarget(error.to_string()))?;
    cache
        .put(
            &keys::realtime_call(&request.target.provider.entity.id, &request.scope, id),
            bytes,
            CALL_TTL,
        )
        .await?;
    Ok(())
}

pub(crate) fn call_id<B>(wire: &WireRequest<B>) -> CoreResult<Option<String>> {
    let path_id = match RealtimeRoute::from_path(&wire.path) {
        Some(
            RealtimeRoute::Live { call_id: Some(id) }
            | RealtimeRoute::HangupCall { call_id: id }
            | RealtimeRoute::LiveSession {
                session_id: Some(id),
            }
            | RealtimeRoute::HangupLiveSession { session_id: id },
        ) => Some(
            percent_encoding::percent_decode_str(id)
                .decode_utf8()
                .map_err(|e| CoreError::InvalidTarget(e.to_string()))?
                .into_owned(),
        ),
        _ => None,
    };
    let ids: Vec<_> = form_urlencoded::parse(wire.query.as_deref().unwrap_or("").as_bytes())
        .filter(|(k, _)| k == "call_id")
        .map(|(_, v)| v.into_owned())
        .collect();
    if ids.len() > 1
        || path_id
            .as_ref()
            .zip(ids.first())
            .is_some_and(|(a, b)| a != b)
    {
        return Err(CoreError::InvalidTarget(
            "conflicting realtime call IDs".into(),
        ));
    }
    let id = path_id.or_else(|| ids.into_iter().next());
    if id
        .as_ref()
        .is_some_and(|s| s.is_empty() || s.contains('/') || matches!(s.as_str(), "." | ".."))
    {
        return Err(CoreError::InvalidTarget("invalid realtime call ID".into()));
    }
    Ok(id)
}
impl<C> Core<C> {
    /// Recover a continuation's model before application admission, so a
    /// model-less sideband URL cannot evade current model permissions.
    pub async fn realtime_request_model(
        &self,
        scope: &str,
        operation: Operation,
        path: &str,
        query: Option<&str>,
        provider: Option<&str>,
    ) -> CoreResult<Option<(String, Option<String>)>> {
        let snapshot = self.snapshot();
        let providers: Vec<_> = snapshot
            .providers
            .keys()
            .filter(|id| provider.is_none_or(|provider| provider == id.as_str()))
            .map(String::as_str)
            .collect();
        let Some(bindings) = self
            .realtime_bindings(scope, operation, path, query, &providers)
            .await?
        else {
            return Ok(None);
        };
        if bindings.len() != 1 {
            return Err(CoreError::Forbidden(
                "voice session is not uniquely bound in this scope",
            ));
        }
        let provider_id = bindings.into_keys().next().expect("one binding");
        let wire = WireRequest {
            method: http::Method::GET,
            path: path.to_owned(),
            query: query.map(str::to_owned),
            headers: Default::default(),
            body: (),
        };
        let id = call_id(&wire)?.expect("continuation has an ID");
        let entry = self
            .cache
            .get(&keys::realtime_call(&provider_id, scope, &id))
            .await?
            .ok_or(CoreError::Forbidden("voice session binding expired"))?;
        let binding: CallBinding = serde_json::from_slice(&entry.value)
            .map_err(|_| CoreError::InvalidTarget("corrupt realtime call binding".into()))?;
        let model = binding.requested_model.or_else(|| {
            binding
                .model
                .map(|model| format!("{}/{model}", snapshot.providers[&provider_id].entity.name))
        });
        Ok(Some((provider_id, model)))
    }

    /// Resolve an existing call before a multi-provider failover walk. Execution
    /// still checks the same binding and permitted credential set again.
    pub(crate) async fn realtime_bindings(
        &self,
        scope: &str,
        operation: Operation,
        path: &str,
        query: Option<&str>,
        providers: &[&str],
    ) -> CoreResult<Option<std::collections::BTreeMap<String, Option<String>>>> {
        if !matches!(
            operation,
            Operation::ConnectRealtime
                | Operation::ConnectLive
                | Operation::HangupRealtimeCall
                | Operation::HangupLiveSession
        ) {
            return Ok(None);
        }
        let wire = WireRequest {
            method: http::Method::GET,
            path: path.to_owned(),
            query: query.map(str::to_owned),
            headers: Default::default(),
            body: (),
        };
        let Some(id) = call_id(&wire)? else {
            return Ok(None);
        };
        let live = matches!(
            operation,
            Operation::ConnectLive | Operation::HangupLiveSession
        );
        let mut bindings = std::collections::BTreeMap::new();
        for &provider in providers {
            if let Some(entry) = self
                .cache
                .get(&keys::realtime_call(provider, scope, &id))
                .await?
            {
                let binding: CallBinding = serde_json::from_slice(&entry.value).map_err(|_| {
                    CoreError::InvalidTarget("corrupt realtime call binding".into())
                })?;
                if binding.live == live {
                    bindings.insert(provider.to_owned(), Some(binding.credential_id));
                }
            }
        }
        Ok(Some(bindings))
    }

    pub(crate) async fn remember_realtime_call(
        &self,
        request: &RequestContext,
        credential_id: &str,
        response: &mut WireResponse<HttpBody>,
        limits: CodecLimits,
    ) -> CoreResult<()> {
        if !matches!(
            request.operation.operation,
            Operation::CreateRealtimeCall | Operation::CreateLiveSession
        ) {
            return Ok(());
        }
        let live = request.operation.operation == Operation::CreateLiveSession;
        let live_id = if live {
            let body = std::mem::replace(&mut response.body, HttpBody::Bytes(Default::default()));
            let bytes = read_http_body(body, limits)
                .await
                .map_err(|error| CoreError::InvalidTarget(error.to_string()))?;
            let value: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|error| CoreError::InvalidTarget(error.to_string()))?;
            response.body = HttpBody::Bytes(bytes);
            Some(
                value
                    .pointer("/session/id")
                    .and_then(serde_json::Value::as_str)
                    .filter(|id| !id.is_empty() && !id.contains('/') && !matches!(*id, "." | ".."))
                    .ok_or_else(|| {
                        CoreError::InvalidTarget(
                            "successful Live creation omitted session.id".into(),
                        )
                    })?
                    .to_owned(),
            )
        } else {
            None
        };
        let location = if let Some(id) = live_id.as_deref() {
            id
        } else {
            response
                .headers
                .get(http::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| {
                    CoreError::InvalidTarget("successful realtime call omitted Location".into())
                })?
        };
        let path = location
            .split('?')
            .next()
            .unwrap_or(location)
            .trim_end_matches('/');
        let id = path
            .rsplit('/')
            .next()
            .filter(|id| {
                live || id.starts_with("rtc_")
                    || (id.len() == 36 && id.bytes().filter(|b| *b == b'-').count() == 4)
            })
            .ok_or_else(|| CoreError::InvalidTarget("invalid realtime Location call ID".into()))?;
        let id = percent_encoding::percent_decode_str(id)
            .decode_utf8()
            .map_err(|e| CoreError::InvalidTarget(e.to_string()))?;
        let value = serde_json::to_vec(&CallBinding {
            credential_id: credential_id.into(),
            model: request.target.upstream_model.clone(),
            requested_model: request.attribution.model.clone(),
            live,
        })
        .map_err(|e| CoreError::InvalidTarget(e.to_string()))?;
        self.cache
            .put(
                &keys::realtime_call(&request.target.provider.entity.id, &request.scope, &id),
                value,
                CALL_TTL,
            )
            .await?;
        Ok(())
    }
    pub(crate) async fn bind_realtime_continuation(
        &self,
        context: Arc<RequestContext>,
        wire: &WireRequest<()>,
    ) -> CoreResult<Arc<RequestContext>> {
        if !matches!(
            context.operation.operation,
            Operation::ConnectRealtime
                | Operation::ConnectLive
                | Operation::HangupRealtimeCall
                | Operation::HangupLiveSession
        ) {
            return Ok(context);
        }
        let Some(id) = call_id(wire)? else {
            if matches!(
                context.operation.operation,
                Operation::HangupRealtimeCall | Operation::HangupLiveSession
            ) {
                return Err(CoreError::InvalidTarget(
                    "hangup requires a bound session ID".into(),
                ));
            }
            return Ok(context);
        };
        let key = keys::realtime_call(&context.target.provider.entity.id, &context.scope, &id);
        let entry = self.cache.get(&key).await?.ok_or(CoreError::Forbidden(
            "realtime call is not bound in this scope",
        ))?;
        let binding: CallBinding = serde_json::from_slice(&entry.value)
            .map_err(|_| CoreError::InvalidTarget("corrupt realtime call binding".into()))?;
        if binding.live
            != matches!(
                context.operation.operation,
                Operation::ConnectLive | Operation::HangupLiveSession
            )
        {
            return Err(CoreError::Forbidden(
                "session belongs to another voice protocol",
            ));
        }
        let credential = context
            .target
            .credentials
            .iter()
            .find(|c| c.id == binding.credential_id)
            .cloned()
            .ok_or(CoreError::Forbidden(
                "realtime credential is outside the permitted set",
            ))?;
        if context
            .target
            .upstream_model
            .as_ref()
            .is_some_and(|m| Some(m) != binding.model.as_ref())
        {
            return Err(CoreError::InvalidTarget(
                "realtime continuation model differs from bound model".into(),
            ));
        }
        if form_urlencoded::parse(wire.query.as_deref().unwrap_or("").as_bytes())
            .any(|(k, v)| k == "model" && Some(v.as_ref()) != binding.model.as_deref())
        {
            return Err(CoreError::InvalidTarget(
                "realtime query model differs from bound model".into(),
            ));
        }
        let mut next = (*context).clone();
        next.target.credentials = vec![credential];
        next.target.upstream_model = binding.model;
        // Session assignment must never rotate an existing call onto another account.
        next.session = None;
        Ok(Arc::new(next))
    }
}

impl<C: gproxy_seaorm::BatchConnectionTrait + Send + Sync + 'static> Core<C> {
    /// Dispatch a native Realtime, legacy Live or public GPT-Live upgrade.
    /// The host supplies an already admitted realtime context and WebSocket upgrade.
    pub async fn connect_realtime_path(
        &self,
        context: Arc<RequestContext>,
        wire: WireRequest<()>,
    ) -> CoreResult<crate::WebSocketExecution> {
        match RealtimeRoute::from_path(&wire.path) {
            Some(RealtimeRoute::Connect | RealtimeRoute::Live { .. }) => {
                self.connect_realtime(context, wire).await
            }
            Some(RealtimeRoute::LiveSession { .. }) => self.connect_live(context, wire).await,
            _ => Err(CoreError::InvalidTarget(
                "not a realtime WebSocket route".into(),
            )),
        }
    }
}

/// Shared atomic response claims, applied before pricing, budgets and observers.
/// The retention window covers every admitted call continuation. The cache must
/// be shared by gateway instances; per-process caches only deduplicate locally.
#[derive(Clone)]
pub(crate) struct SettlementDedup {
    cache: Arc<dyn gproxy_cache::Cache>,
    call_id: Option<String>,
}
impl SettlementDedup {
    pub(crate) fn new(cache: Arc<dyn gproxy_cache::Cache>, call_id: Option<String>) -> Self {
        Self { cache, call_id }
    }
    pub(crate) async fn filter(
        &self,
        request: &RequestContext,
        report: &mut crate::UsageReport,
    ) -> CoreResult<()> {
        let mut exchanges = Vec::new();
        for mut exchange in std::mem::take(&mut report.exchanges) {
            if exchange.usage.responses.is_empty() {
                exchanges.push(exchange);
                continue;
            }
            let mut accepted = Vec::new();
            for mut response in std::mem::take(&mut exchange.usage.responses) {
                let key = keys::realtime_response(
                    &exchange.provider_id,
                    &request.scope,
                    &exchange.credential_id,
                    self.call_id.as_deref(),
                    &response.id,
                );
                if response
                    .usage
                    .dimensions
                    .get("cumulative_audio_seconds")
                    .is_some_and(|v| v == "true")
                {
                    let Some(reported) = response.usage.metrics.get("audio_seconds").copied()
                    else {
                        continue;
                    };
                    let Some(delta) = self.claim_duration(&key, reported).await? else {
                        continue;
                    };
                    response.usage.metrics.insert("audio_seconds".into(), delta);
                    for attempt in &mut response.usage.attempts {
                        attempt.usage.metrics.insert("audio_seconds".into(), delta);
                    }
                    response
                        .usage
                        .dimensions
                        .insert("settlement_id".into(), format!("{key}:{reported}"));
                    accepted.push(response);
                    continue;
                }
                let claim = gproxy_cache::Replacement {
                    value: exchange.capture_id.as_bytes().to_vec(),
                    ttl: CALL_TTL,
                };
                match self.cache.compare_exchange(&key, None, Some(claim)).await? {
                    gproxy_cache::CasOutcome::Applied(_) => {
                        // Also expose a stable key for durable/idempotent host ledgers.
                        response
                            .usage
                            .dimensions
                            .insert("settlement_id".into(), key);
                        accepted.push(response);
                    }
                    gproxy_cache::CasOutcome::Conflict => {}
                }
            }
            if accepted.is_empty() {
                continue;
            }
            let completeness = exchange.usage.completeness;
            exchange.usage = gproxy_channel::channel::NormalizedUsage::aggregate(
                accepted.iter().map(|r| r.usage.as_ref()),
            );
            exchange.usage.completeness = completeness;
            exchange.usage.attempts = accepted
                .iter()
                .flat_map(|response| response.usage.attempts.clone())
                .collect();
            exchange.usage.responses = accepted;
            exchanges.push(exchange);
        }
        report.exchanges = exchanges;
        Ok(())
    }

    /// Sidebands may end before the primary session. Claim only the increase
    /// over the previously observed cumulative duration, on the shared cache.
    async fn claim_duration(
        &self,
        key: &str,
        reported: rust_decimal::Decimal,
    ) -> CoreResult<Option<rust_decimal::Decimal>> {
        loop {
            let entry = self.cache.get(key).await?;
            let previous = match &entry {
                Some(entry) => std::str::from_utf8(&entry.value)
                    .ok()
                    .and_then(|value| value.parse::<rust_decimal::Decimal>().ok())
                    .ok_or(gproxy_cache::CacheError::Corrupt)?,
                None => rust_decimal::Decimal::ZERO,
            };
            if entry.is_some() && reported <= previous {
                return Ok(None);
            }
            let replacement = gproxy_cache::Replacement {
                value: reported.to_string().into_bytes(),
                ttl: CALL_TTL,
            };
            if matches!(
                self.cache
                    .compare_exchange(key, entry.map(|entry| entry.version), Some(replacement))
                    .await?,
                gproxy_cache::CasOutcome::Applied(_)
            ) {
                return Ok(Some(
                    reported
                        .saturating_sub(previous)
                        .max(rust_decimal::Decimal::ZERO),
                ));
            }
        }
    }
}
