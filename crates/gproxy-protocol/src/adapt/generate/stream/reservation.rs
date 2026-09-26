use super::super::{Endpoint, GenerationIdentity, GenerationStateAccess};
use crate::{
    capability::{CasResult, StateStore, StateWrite, Version},
    codec::{self, CodecLimits},
    transform::TransformError,
    wire::DeclaredFields,
};
use serde::Serialize;

#[derive(Clone)]
pub(super) struct Reservation {
    key: String,
    payload: Vec<u8>,
    in_flight: bool,
    version: Option<Version>,
    failed: bool,
    ws_lane: Option<Option<String>>,
}

impl Reservation {
    pub fn new<
        C: Serialize + Clone + DeclaredFields,
        N: Serialize + Clone + DeclaredFields,
        S: StateStore,
    >(
        original: &C,
        native: &N,
        endpoint: &Endpoint,
        identities: &GenerationIdentity,
        state: &GenerationStateAccess<'_, S>,
        limits: CodecLimits,
    ) -> Result<Self, TransformError> {
        let original = codec::encode_json(&original.clone().into_declared(), limits)
            .map_err(super::codec_error)?;
        let prepared = codec::encode_json(&native.clone().into_declared(), limits)
            .map_err(super::codec_error)?;
        let headers: Vec<_> = endpoint
            .headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_bytes()))
            .collect();
        // This is a binding journal of the two concrete declared requests, not an IR.
        let payload = serde_json::to_vec(&serde_json::json!({
            "schema":1, "request":std::str::from_utf8(&original).map_err(|e| super::invalid(e.to_string()))?,
            "prepared":std::str::from_utf8(&prepared).map_err(|e| super::invalid(e.to_string()))?,
            "path":endpoint.path, "query":endpoint.query, "headers":headers,
            "request_namespace":identities.request.namespace(), "response_namespace":identities.response.namespace(),
            "request_policy":identities.request_policy, "response_policy":identities.response_policy,
            "target":state.target, "conversation":state.conversation_key, "expires_at":state.expires_at,
        }))?;
        if payload.len() as u64 > state.store.limits().write_bytes {
            return Err(super::limit("stream reservation exceeds state write limit"));
        }
        Ok(Self {
            key: format!(
                "stream-invoke:{}:{}:{}:{}",
                state.conversation_key.len(),
                state.conversation_key,
                identities.request.namespace().hex(),
                identities.response.namespace().hex()
            ),
            payload,
            in_flight: false,
            version: None,
            failed: false,
            ws_lane: None,
        })
    }
    pub async fn connection<S: StateStore>(
        namespace: crate::transform::identity::IdNamespace,
        request: &crate::WireRequest<()>,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let headers: Vec<_> = request
            .headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_bytes()))
            .collect();
        let payload = serde_json::to_vec(
            &serde_json::json!({ "transport": "responses_websocket_connection", "namespace": namespace, "method": request.method.as_str(), "path": request.path, "query": request.query, "headers": headers, "target": state.target, "conversation": state.conversation_key, "expires_at": state.expires_at }),
        )?;
        if payload.len() as u64 > state.store.limits().write_bytes {
            return Err(super::limit(
                "WebSocket connection binding exceeds state write limit",
            ));
        }
        let mut marker = Self {
            key: format!(
                "ws-connect:{}:{}:{}",
                state.conversation_key.len(),
                state.conversation_key,
                namespace.hex()
            ),
            payload,
            in_flight: false,
            version: None,
            failed: false,
            ws_lane: None,
        };
        marker.reserve(state).await?;
        Ok(marker)
    }
    pub fn websocket<S: StateStore>(
        &mut self,
        lane: Option<&str>,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<(), TransformError> {
        let lane = lane.map(str::to_owned);
        if let Some(known) = &self.ws_lane {
            if known != &lane {
                return Err(super::conflict("reserved WebSocket lane changed"));
            }
            return Ok(());
        }
        if self.in_flight || self.version.is_some() {
            return Err(super::conflict(
                "HTTP reservation cannot become a WebSocket turn",
            ));
        }
        self.payload = serde_json::to_vec(
            &serde_json::json!({ "transport": "responses_websocket", "stream_id": lane, "binding": serde_json::from_slice::<serde_json::Value>(&self.payload)? }),
        )?;
        if self.payload.len() as u64 > state.store.limits().write_bytes {
            return Err(super::limit(
                "WebSocket reservation exceeds state write budget",
            ));
        }
        self.ws_lane = Some(lane);
        Ok(())
    }
    pub fn is_websocket(&self) -> bool {
        self.ws_lane.is_some()
    }
    pub async fn preparation<S: StateStore>(
        &self,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let mut binding = Self {
            key: format!("stream-prepare:{}", self.key),
            payload: self.payload.clone(),
            in_flight: false,
            version: None,
            failed: false,
            ws_lane: None,
        };
        binding.reserve(state).await?;
        Ok(binding)
    }
    pub async fn verify<S: StateStore>(
        &self,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<(), TransformError> {
        let expected = self
            .version
            .as_ref()
            .ok_or_else(|| super::missing("preparation binding was not acknowledged"))?;
        let entry = state
            .store
            .get(state.scope, &self.key)
            .await?
            .ok_or_else(|| {
                super::missing("preparation binding is absent in supplied state scope")
            })?;
        if entry.payload.len() as u64 > state.store.limits().read_bytes {
            return Err(super::limit("preparation binding exceeds state read limit"));
        }
        if &entry.version != expected
            || entry.payload.as_ref() != self.payload
            || entry.expires_at != Some(state.expires_at)
        {
            return Err(super::conflict("preparation binding changed"));
        }
        Ok(())
    }
    pub async fn reserve<S: StateStore>(
        &mut self,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<(), TransformError> {
        if self.failed {
            return Err(super::conflict("stream invocation reservation failed"));
        }
        if self.in_flight || self.version.is_some() {
            let entry = state
                .store
                .get(state.scope, &self.key)
                .await?
                .ok_or_else(|| {
                    super::missing("unacknowledged stream reservation has no durable result")
                })?;
            if entry.payload.len() as u64 > state.store.limits().read_bytes {
                return Err(super::limit("stream reservation exceeds state read limit"));
            }
            if entry.payload.as_ref() != self.payload
                || entry.expires_at != Some(state.expires_at)
                || self.version.as_ref().is_some_and(|v| v != &entry.version)
            {
                return Err(super::conflict("stream invocation reservation changed"));
            }
            self.version = Some(entry.version);
            self.in_flight = false;
            return Ok(());
        }
        self.in_flight = true;
        match state
            .store
            .compare_exchange(
                state.scope,
                &self.key,
                None,
                Some(StateWrite {
                    payload: self.payload.clone().into(),
                    expires_at: Some(state.expires_at),
                }),
            )
            .await?
        {
            CasResult::Applied(Some(version)) => {
                self.version = Some(version);
                self.in_flight = false;
                Ok(())
            }
            _ => {
                self.failed = true;
                Err(super::conflict(
                    "invocation namespace already reserved; cannot replay POST",
                ))
            }
        }
    }
}
