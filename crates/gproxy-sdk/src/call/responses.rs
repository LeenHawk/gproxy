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
