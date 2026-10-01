use super::*;
use gproxy_core::{CoreResult, ResponsesHttpExecutor, ResponsesSession};
use gproxy_protocol::{WireResponse, capability::CapabilityFuture};

pub(super) struct HttpExecutor<C> {
    pub gproxy: Gproxy<C>,
    pub contexts: Vec<Arc<RequestContext>>,
    pub affinity: Option<RouteAffinity>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> ResponsesHttpExecutor for HttpExecutor<C> {
    fn send<'a>(
        &'a self,
        session: &'a ResponsesSession,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, CoreResult<WireResponse<HttpBody>>> {
        Box::pin(async move {
            let WireRequest {
                method,
                path,
                query,
                headers,
                body,
            } = request;
            let HttpBody::Bytes(body) = body else {
                return Err(CoreError::InvalidTarget(
                    "Responses segment must be buffered".into(),
                ));
            };
            let binding = session.committed_http_binding();
            let mut held = None;
            let mut last = CoreError::NoUsableCredential;
            for context in &self.contexts {
                if binding
                    .as_ref()
                    .is_some_and(|bound| bound.provider_id != context.target.provider.entity.id)
                {
                    continue;
                }
                let request = WireRequest {
                    method: method.clone(),
                    path: path.clone(),
                    query: query.clone(),
                    headers: headers.clone(),
                    body: HttpBody::Bytes(body.clone()),
                };
                match self
                    .gproxy
                    .core()
                    .send_responses_http(session, context.clone(), request)
                    .await
                {
                    Ok(response) if failover_status(response.status) && binding.is_none() => {
                        // Close observation before the next provider checks the
                        // pending cost; keep the final rejection if none can run.
                        let body = match response.body {
                            HttpBody::Bytes(body) => body,
                            HttpBody::Stream(mut stream) => {
                                let mut bytes = Vec::new();
                                while let Some(chunk) = stream.next().await {
                                    bytes.extend_from_slice(&chunk.map_err(|e| {
                                        CoreError::Channel(
                                            gproxy_channel::ChannelError::InvalidResponse(
                                                e.to_string(),
                                            ),
                                        )
                                    })?);
                                }
                                Bytes::from(bytes)
                            }
                        };
                        held = Some(WireResponse {
                            status: response.status,
                            headers: response.headers,
                            body: HttpBody::Bytes(body),
                        });
                    }
                    Ok(response) => {
                        if response.status.is_success()
                            && let Some(affinity) = &self.affinity
                        {
                            affinity.commit(&self.gproxy, &context.target).await;
                        }
                        return Ok(response);
                    }
                    Err(
                        error @ (CoreError::NoUsableCredential
                        | CoreError::CredentialDead { .. }
                        | CoreError::RefreshContended { .. }),
                    ) => last = error,
                    // In particular, do not replay an ambiguous transport error.
                    Err(error) => return Err(error),
                }
            }
            held.ok_or(last)
        })
    }
}

/// Ordinary SDK connections admit their turns automatically. Hosts that own
/// admission use open_responses/begin_responses_turn themselves.
pub(super) async fn connect<C: BatchConnectionTrait + Send + Sync + 'static>(
    mut builder: ConnectBuilder<'_, C>,
) -> SdkResult<WebSocketExecution> {
    use futures_util::SinkExt;
    use gproxy_protocol::connection::{WebSocket, WsFrame};
    if builder.options.session.is_none() {
        builder.options.session = Some(
            session::extract(&builder.request.headers, None, builder.operation).unwrap_or_else(
                || SessionIdentity {
                    id: random_id(),
                    source: SessionSource::Generic,
                    field: Some("responses.chain".into()),
                    agent_session_id: None,
                },
            ),
        );
    }
    let session = ResponsesSession::new();
    let automatic = Automatic {
        gproxy: builder.gproxy.clone(),
        operation: builder.operation,
        headers: builder.request.headers.clone(),
        options: builder.options.clone(),
        session: session.clone(),
        request: None,
    };
    let execution = builder.open_responses(session).await?;
    Ok(execution.map_response(|connection| match connection {
        UpstreamConnection::Connected { handshake, socket } => {
            let (errors, receive) = tokio::sync::mpsc::channel(16);
            let incoming = futures_util::stream::unfold((socket.incoming, receive), |(mut incoming, mut receive)| async move {
                let frame = tokio::select! {
                    frame = incoming.next() => frame?,
                    Some(error) = receive.recv() => Ok(error),
                };
                Some((frame, (incoming, receive)))
            });
            let outgoing = futures_util::sink::unfold((socket.outgoing, automatic, errors), |(mut outgoing, mut automatic, errors), frame| async move {
                if let WsFrame::Text(text) = &frame {
                    let result = match serde_json::from_str::<Value>(text) {
                        Ok(value) => automatic.admit(&value).await,
                        Err(error) => Err(SdkError::invalid(error.to_string())),
                    };
                    if let Err(error) = result {
                        let lane = serde_json::from_str::<Value>(text).ok().and_then(|value| value.get("stream_id").cloned());
                        let event = serde_json::json!({"type":"error","status":error.status_code(),"stream_id":lane,"error":{"type":"invalid_request_error","message":error.to_string()}});
                        let _ = errors.send(WsFrame::Text(event.to_string())).await;
                        return Ok((outgoing, automatic, errors));
                    }
                }
                outgoing.send(frame).await?;
                Ok((outgoing, automatic, errors))
            });
            UpstreamConnection::Connected { handshake, socket: WebSocket { incoming: Box::pin(incoming), outgoing: Box::pin(outgoing) } }
        }
        rejected => rejected,
    }))
}

struct Automatic<C> {
    gproxy: Gproxy<C>,
    operation: OperationKey,
    headers: HeaderMap,
    options: Options,
    session: ResponsesSession,
    request: Option<Value>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Automatic<C> {
    async fn admit(&mut self, value: &Value) -> SdkResult<()> {
        let (pending, body) = match value["type"].as_str() {
            Some("response.create") => (false, value.clone()),
            Some("response.steer") => {
                let mut body = self
                    .request
                    .clone()
                    .ok_or_else(|| SdkError::invalid("no response to steer"))?;
                body["previous_response_id"] = value["previous_response_id"].clone();
                (true, body)
            }
            _ => return Ok(()),
        };
        let mut options = self.options.clone();
        options.request_id = Some(random_id());
        if (!self.session.is_http_bridge() || body["previous_response_id"].is_string())
            && let Some(binding) = self.session.binding()
        {
            options.providers = Some(BTreeSet::from([binding.provider_id]));
            options.credentials = Some(BTreeSet::from([binding.credential_id]));
        }
        let builder = ConnectBuilder {
            gproxy: &self.gproxy,
            operation: self.operation,
            options,
            request: WireRequest {
                method: http::Method::GET,
                path: "/v1/responses".into(),
                query: None,
                headers: self.headers.clone(),
                body: (),
            },
        };
        let _ = builder
            .begin_responses_turn(&self.session, &body, value["stream_id"].as_str(), pending)
            .await?;
        if !pending {
            self.request = Some(body);
        }
        Ok(())
    }
}
