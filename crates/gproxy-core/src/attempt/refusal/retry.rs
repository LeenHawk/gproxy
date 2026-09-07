use bytes::Bytes;
use gproxy_channel_api::{Disposition, PrepareCtx, ResponseView};
use serde_json::{Value, json};

use crate::api::Core;
use crate::boundary::ByteStream;
use crate::error::CoreError;
use crate::funnel::FunnelCtx;
use crate::host::{Capture, CaptureSink, Host, UpstreamTransport};

use super::{Replay, credit, meter::Meter};

pub(super) struct Runner<H: Host> {
    pub core: Core<H>,
    pub replay: Replay,
    pub facts: FunnelCtx,
    pub meter: Meter,
    pub sent: u32,
    pub prefix: Vec<Value>,
    pub boundaries: Vec<Value>,
    pub wire_iterations: Vec<Value>,
}

impl<H: Host> Runner<H> {
    pub(super) fn new(core: &Core<H>, facts: &FunnelCtx, replay: Replay) -> Self {
        Self {
            core: core.clone(),
            facts: facts.clone(),
            replay,
            meter: Meter::new(),
            sent: 1,
            prefix: Vec::new(),
            boundaries: Vec::new(),
            wire_iterations: Vec::new(),
        }
    }

    pub(super) async fn next(
        &mut self,
        refused: &Value,
    ) -> Result<Option<http::Response<ByteStream>>, CoreError> {
        if self.sent >= self.replay.budget || self.replay.policy.tried.len() >= 4 {
            return Ok(None);
        }
        let Some(entry) = self.replay.policy.next(
            self.core
                .channels
                .get(&self.facts.target.provider.channel)
                .expect("channel"),
            refused,
            &self.facts.target.upstream_model,
        ) else {
            return Ok(None);
        };
        let model = entry["model"]
            .as_str()
            .expect("validated candidate")
            .to_owned();
        let mut exact: Value = serde_json::from_slice(&self.replay.body)
            .map_err(|error| CoreError::Transform(error.to_string()))?;
        let original = exact.clone();
        let object = exact.as_object_mut().expect("validated request object");
        object.remove("fallbacks");
        object.remove("fallback_credit_token");
        object.insert("model".into(), json!(model));
        for field in ["max_tokens", "thinking", "output_config", "speed"] {
            if let Some(value) = entry.get(field) {
                object.insert(field.into(), value.clone());
            }
        }
        let prompt_matches = credit::PROMPT_FIELDS
            .iter()
            .all(|field| original.get(*field) == exact.get(*field));
        let tools = credit::completed_tools(refused);
        let token = refused
            .pointer("/stop_details/fallback_credit_token")
            .and_then(Value::as_str)
            .filter(|_| self.replay.policy.capabilities.credit && prompt_matches);
        if tools && token.is_none() {
            return Err(CoreError::Transform(
                "fallback would re-execute completed server tools without a redeemable credit"
                    .into(),
            ));
        }
        if let Some(token) = token {
            exact["fallback_credit_token"] = json!(token);
        }
        let mut continuing = token.is_some()
            && refused.pointer("/stop_details/fallback_has_prefill_claim")
                != Some(&Value::Bool(false));
        let mut body = if continuing {
            credit::continuation(&exact, refused)?
        } else {
            exact.clone()
        };
        let mut carrying_credit = token.is_some();
        let created = web_time::Instant::now();
        let from = refused
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(&self.facts.target.upstream_model)
            .to_owned();
        loop {
            if carrying_credit && created.elapsed() >= std::time::Duration::from_secs(300) {
                if tools {
                    return Err(CoreError::Transform(
                        "fallback credit expired after server tools ran".into(),
                    ));
                }
                exact
                    .as_object_mut()
                    .expect("object")
                    .remove("fallback_credit_token");
                body = exact.clone();
                continuing = false;
                carrying_credit = false;
            }
            let response = match self.send(&model, &body).await {
                Ok(response) => response,
                Err(CoreError::QuotaExceeded | CoreError::RateLimited { .. }) => return Ok(None),
                Err(error) => return Err(error),
            };
            if response.status() != http::StatusCode::BAD_REQUEST {
                if response.status().is_success() {
                    self.replay.policy.tried.insert(model.clone());
                    if continuing {
                        self.prefix.extend(
                            refused
                                .get("content")
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default(),
                        );
                    } else {
                        self.prefix.clear();
                    }
                    let boundary =
                        json!({"type":"fallback","from":{"model":from},"to":{"model":model}});
                    self.boundaries.push(boundary.clone());
                    if continuing {
                        self.prefix.push(boundary);
                    } else {
                        self.prefix.clone_from(&self.boundaries);
                    }
                }
                return Ok(Some(response));
            }
            let response = crate::attempt::body::collect(response)
                .await
                .map_err(|error| CoreError::Transport(error.error))?;
            self.capture(
                response.status(),
                response.headers(),
                response.body().clone(),
            )
            .await;
            let message = credit::message(response.body());
            if self.sent >= self.replay.budget {
                return Ok(Some(as_stream(response)));
            }
            if carrying_credit && message.contains("redemption temporarily unavailable") {
                self.core.host.wait(std::time::Duration::from_secs(1)).await;
                continue;
            }
            if continuing {
                body = exact.clone();
                continuing = false;
            } else if carrying_credit && message.contains("fallback_credit_token") && !tools {
                exact
                    .as_object_mut()
                    .expect("object")
                    .remove("fallback_credit_token");
                body = exact.clone();
                carrying_credit = false;
            } else {
                return Ok(Some(as_stream(response)));
            }
        }
    }

    async fn send(
        &mut self,
        model: &str,
        body: &Value,
    ) -> Result<http::Response<ByteStream>, CoreError> {
        let channel = self
            .core
            .channels
            .get(self.facts.target.provider.channel.as_str())
            .expect("prepared channel");
        let mut target = self.facts.target.clone();
        target.upstream_model = model.into();
        target.provider.settings["claude_fallback_mode"] = json!("off");
        let body = Bytes::from(serde_json::to_vec(body).expect("JSON serializes"));
        self.core
            .host
            .admit_retry(&self.facts.request_id, &target, &body, self.facts.settle)
            .await?;
        let credential = crate::execution::credential::load_fresh(
            self.core.host.as_ref(),
            channel,
            target.credential,
            &target.provider,
        )
        .await?;
        let mut headers = self.replay.headers.clone();
        if let Some(betas) = headers
            .get("anthropic-beta")
            .and_then(|value| value.to_str().ok())
        {
            let betas = betas
                .split(',')
                .filter(|value| !value.trim().starts_with("server-side-fallback-"))
                .collect::<Vec<_>>()
                .join(",");
            headers.insert("anthropic-beta", betas.parse().expect("valid beta header"));
        }
        let mut prepared = channel.prepare(PrepareCtx {
            key: self.facts.key.expect("Messages operation"),
            session_id: self.replay.session_id.as_deref(),
            stream: self.facts.key.expect("Messages").operation()
                == gproxy_protocol::Operation::StreamGenerateContent,
            method: &self.replay.method,
            path: &self.replay.path,
            query: self.replay.query.as_deref(),
            headers: &headers,
            body: &body,
            upstream_model: model,
            provider_settings: &target.provider.settings,
            secret: &credential.secret,
        })?;
        crate::fingerprint::apply_prepared(&mut prepared, &target.provider)?;
        self.replay.capture(&prepared.request);
        self.facts.target = target;
        self.facts.credential_version = Some(credential.version);
        self.facts.upstream_started_at_ms = Some(crate::quota::now_ms());
        self.facts.upstream_url = Some(prepared.request.uri().to_string());
        self.facts.request_body = prepared.request.body().clone();
        self.facts.request_headers = Some(prepared.request.headers().clone());
        if channel.quota_capabilities(&credential.secret).is_some() {
            self.core
                .host
                .begin_credential_usage(
                    &format!("{}:attempt:{}", self.facts.request_id, self.meter.len()),
                    &self.facts.target,
                    self.facts.upstream_started_at_ms.expect("send time"),
                )
                .await?;
        }
        self.sent += 1;
        let response = match self.core.host.transport().send(prepared.request).await {
            Ok(response) => response,
            Err(error) => {
                self.meter.reject(
                    model.into(),
                    self.facts.upstream_started_at_ms.expect("send time"),
                    0,
                );
                crate::funnel::error::attempt_transport(
                    self.core.host.as_ref(),
                    &self.facts,
                    &error,
                )
                .await;
                return Err(error.into());
            }
        };
        if !response.status().is_success() {
            self.meter.reject(
                model.into(),
                self.facts.upstream_started_at_ms.expect("send time"),
                response.status().as_u16(),
            );
        }
        let disposition = channel.classify(ResponseView {
            status: response.status(),
            headers: response.headers(),
            body: &[],
        });
        crate::funnel::health::response(
            self.core.host.as_ref(),
            channel,
            &self.facts,
            disposition,
            response.status(),
            response.headers(),
        )
        .await;
        if disposition == Disposition::CredentialDead {
            tracing::warn!(request_id = %self.facts.request_id, "fallback credential was rejected");
        }
        Ok(response)
    }

    pub(super) async fn capture(
        &self,
        status: http::StatusCode,
        headers: &http::HeaderMap,
        body: Bytes,
    ) {
        self.core
            .host
            .capture()
            .record(&Capture {
                request_id: self.facts.request_id.clone(),
                provider_id: Some(self.facts.target.provider.id),
                credential_id: Some(self.facts.target.credential),
                upstream_url: self.facts.upstream_url.clone(),
                request_method: Some(self.replay.method.clone()),
                request_headers: self.facts.request_headers.clone(),
                request_body: self.replay.body.clone(),
                response_status: Some(status),
                response_headers: Some(headers.clone()),
                response_body: Some(body),
            })
            .await;
    }

    pub(super) fn outward(&self, mut response: Value) -> Value {
        if !self.boundaries.is_empty() {
            let mut content = self.prefix.clone();
            content.extend(
                response
                    .get("content")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
            response["content"] = json!(content);
        }
        if self.wire_iterations.len() > 1 {
            let mut iterations = self.wire_iterations.clone();
            for item in &mut iterations {
                item["type"] = json!("message");
            }
            if let Some(last) = iterations.last_mut() {
                last["type"] = json!("fallback_message");
            }
            response["usage"]["iterations"] = json!(iterations);
        }
        response
    }

    pub(super) fn record_wire(&mut self, response: &Value) {
        if let Some(iterations) = response
            .pointer("/usage/iterations")
            .and_then(Value::as_array)
        {
            self.wire_iterations.extend(iterations.clone());
        } else if let Some(usage) = response.get("usage").filter(|usage| usage.is_object()) {
            let mut usage = usage.clone();
            usage["model"] = response
                .get("model")
                .cloned()
                .unwrap_or_else(|| json!(self.facts.target.upstream_model));
            usage["type"] = json!("message");
            self.wire_iterations.push(usage);
        }
    }

    pub(super) async fn pin(&self, response: &Value) {
        if (self.sent > 1 || self.replay.pinned)
            && response["stop_reason"] != "refusal"
            && let Some(model) = response.get("model").and_then(Value::as_str)
        {
            let channel = self
                .core
                .channels
                .get(&self.facts.target.provider.channel)
                .expect("channel");
            let model = channel.fallback_model(&self.facts.target.upstream_model, model);
            super::pin::save(&self.core, self.replay.pin_key.as_deref(), &model).await;
        }
    }
}

pub(super) fn as_stream(response: http::Response<Bytes>) -> http::Response<ByteStream> {
    let (parts, body) = response.into_parts();
    http::Response::from_parts(
        parts,
        Box::pin(futures_util::stream::once(async move { Ok(body) })) as ByteStream,
    )
}
