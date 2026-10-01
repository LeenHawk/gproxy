//! Realtime route dispatch and scope-isolated call-to-credential continuations.
pub mod request;
use crate::{Core, CoreError, CoreResult, RequestContext, keys};
pub use gproxy_protocol::wire::openai::realtime::RealtimeRoute;
use gproxy_protocol::{Operation, WireRequest};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

const CALL_TTL: Duration = Duration::from_secs(24 * 60 * 60);
#[derive(Serialize, Deserialize)]
struct CallBinding {
    credential_id: String,
    model: Option<String>,
}

pub(crate) fn call_id(wire: &WireRequest<()>) -> CoreResult<Option<String>> {
    let path_id = match RealtimeRoute::from_path(&wire.path) {
        Some(RealtimeRoute::Live { call_id: Some(id) }) => Some(
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
    pub(crate) async fn remember_realtime_call(
        &self,
        request: &RequestContext,
        credential_id: &str,
        headers: &http::HeaderMap,
    ) -> CoreResult<()> {
        if request.operation.operation != Operation::CreateRealtimeCall {
            return Ok(());
        }
        let location = headers
            .get(http::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                CoreError::InvalidTarget("successful realtime call omitted Location".into())
            })?;
        let path = location
            .split('?')
            .next()
            .unwrap_or(location)
            .trim_end_matches('/');
        let id = path
            .rsplit('/')
            .next()
            .filter(|id| {
                id.starts_with("rtc_")
                    || (id.len() == 36 && id.bytes().filter(|b| *b == b'-').count() == 4)
            })
            .ok_or_else(|| CoreError::InvalidTarget("invalid realtime Location call ID".into()))?;
        let id = percent_encoding::percent_decode_str(id)
            .decode_utf8()
            .map_err(|e| CoreError::InvalidTarget(e.to_string()))?;
        let value = serde_json::to_vec(&CallBinding {
            credential_id: credential_id.into(),
            model: request.target.upstream_model.clone(),
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
        if context.operation.operation != Operation::ConnectRealtime {
            return Ok(context);
        }
        let Some(id) = call_id(wire)? else {
            return Ok(context);
        };
        let key = keys::realtime_call(&context.target.provider.entity.id, &context.scope, &id);
        let entry = self.cache.get(&key).await?.ok_or(CoreError::Forbidden(
            "realtime call is not bound in this scope",
        ))?;
        let binding: CallBinding = serde_json::from_slice(&entry.value)
            .map_err(|_| CoreError::InvalidTarget("corrupt realtime call binding".into()))?;
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
    /// Dispatch a native `/realtime`, `/live` or `/live/{call_id}` upgrade.
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
            exchange.usage.responses = accepted;
            exchanges.push(exchange);
        }
        report.exchanges = exchanges;
        Ok(())
    }
}
