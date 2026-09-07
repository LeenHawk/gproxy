use gproxy_channel_api::{TransportError, WsFrame};
use gproxy_protocol::{ContentGenerationKind, OperationKind};
use web_time::Instant;

use super::{ActiveResponse, Classified, Plan, RequestCtx, ResponsesBridge};
use crate::attempt::{self, AdmissionCtx, Egress};
use crate::host::{Host, UpstreamTransport};

impl<H: Host> ResponsesBridge<H> {
    pub(super) async fn connect_native(
        &mut self,
        request: &RequestCtx,
        plan: &Plan,
        classified: &Classified,
    ) -> Result<bool, TransportError> {
        let mut native_plan = plan.clone();
        native_plan.targets.retain(|target| {
            attempt::support(&self.core, target, classified.key)
                .ok()
                .flatten()
                .is_some_and(|support| {
                    support.target.kind()
                        == OperationKind::ContentGeneration(
                            ContentGenerationKind::OpenAiResponsesWebSocket,
                        )
                })
        });
        if native_plan.targets.is_empty() {
            return Ok(false);
        }
        let plan = match self
            .core
            .host
            .admit(&self.identity, request, Some(classified.key), &native_plan)
            .await
        {
            Ok(plan) => plan,
            // HTTP fallback may still have authorized providers.
            Err(crate::error::CoreError::Forbidden(_)) => return Ok(false),
            Err(error) => return Err(super::transport(error)),
        };
        let candidates = plan.targets.iter().take(plan.budget.max_attempts as usize);
        for target in candidates {
            let Ok(attempt::Prepared {
                egress: Egress::WebSocket(upstream_request),
                facts,
                ..
            }) = attempt::prepare(
                &self.core,
                self.control.as_ref(),
                target,
                request,
                classified,
                AdmissionCtx {
                    admitted: true,
                    owner_user_id: Some(self.identity.user_id),
                },
                Instant::now(),
            )
            .await
            else {
                continue;
            };
            let frame = super::request_text(upstream_request.body())?;
            let Ok(mut socket) = self
                .core
                .host
                .transport()
                .open_websocket(*upstream_request)
                .await
            else {
                continue;
            };
            if socket.send(WsFrame::Text(frame)).await.is_err() {
                continue;
            }
            let version = facts
                .credential_version
                .expect("native websocket loaded a credential");
            self.pinned = Some((facts.target.clone(), version));
            self.active = Some(ActiveResponse::new(facts));
            self.native = Some(socket);
            return Ok(true);
        }
        self.core
            .host
            .finish_admission(&request.request_id, None)
            .await;
        Ok(false)
    }

    pub(super) async fn prepare_pinned(
        &mut self,
        request: &RequestCtx,
        plan: &Plan,
        classified: &Classified,
    ) -> Result<(), TransportError> {
        let plan = self
            .core
            .host
            .admit(&self.identity, request, Some(classified.key), plan)
            .await
            .map_err(super::transport)?;
        let prepared = match attempt::prepare(
            &self.core,
            self.control.as_ref(),
            &plan.targets[0],
            request,
            classified,
            AdmissionCtx {
                admitted: true,
                owner_user_id: Some(self.identity.user_id),
            },
            Instant::now(),
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(error) => {
                self.core
                    .host
                    .finish_admission(&request.request_id, None)
                    .await;
                return Err(super::transport(error));
            }
        };
        let attempt::Prepared {
            egress: Egress::WebSocket(frame),
            facts,
            ..
        } = prepared
        else {
            self.core
                .host
                .finish_admission(&request.request_id, None)
                .await;
            return Err(TransportError::Interrupted(
                "pinned channel stopped supporting websocket".into(),
            ));
        };
        if facts.credential_version != self.pinned.as_ref().map(|(_, version)| *version) {
            crate::funnel::complete_stream(
                self.core.host.clone(),
                facts,
                http::StatusCode::SWITCHING_PROTOCOLS,
                None,
                None,
                None,
                crate::usage::Ended::Interrupted,
            )
            .await;
            return Err(TransportError::Interrupted(
                "websocket credential rotated; reconnect required".into(),
            ));
        }
        let sent = self
            .native
            .as_mut()
            .expect("pinned connection has native socket")
            .send(WsFrame::Text(super::request_text(frame.body())?))
            .await;
        if let Err(error) = sent {
            crate::funnel::complete_stream(
                self.core.host.clone(),
                facts,
                http::StatusCode::SWITCHING_PROTOCOLS,
                None,
                None,
                None,
                crate::usage::Ended::Interrupted,
            )
            .await;
            return Err(error);
        }
        self.active = Some(ActiveResponse::new(facts));
        Ok(())
    }
}
