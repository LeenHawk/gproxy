use super::*;
use crate::Error;
use futures_util::{StreamExt, stream};
use gproxy_sdk::Gproxy;
use gproxy_seaorm::BatchConnectionTrait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct Embedded<C> {
    engine: Arc<Gproxy<C>>,
    scope: String,
}
impl<C> Embedded<C> {
    pub fn new(engine: Arc<Gproxy<C>>, scope: impl Into<String>) -> Self {
        Self {
            engine,
            scope: scope.into(),
        }
    }
}
impl<C: BatchConnectionTrait + Send + Sync + 'static> Backend for Embedded<C> {
    fn send(
        &self,
        operation: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        let model = model.map(str::to_owned);
        Box::pin(async move {
            // The HTTP host serves this from the local catalog, before
            // calling upstream. Embedded callers see the same name forms;
            // access policy is the embedding host's responsibility.
            if operation.operation == gproxy_protocol::Operation::ListModels {
                let routing = self.engine.routing();
                let core = self.engine.core().snapshot();
                let mut names: std::collections::BTreeSet<String> = routing
                    .names
                    .iter()
                    .filter(|(_, route_id)| {
                        routing.routes[*route_id]
                            .members
                            .iter()
                            .any(|m| core.providers.contains_key(&m.provider_id))
                    })
                    .map(|(name, _)| name.clone())
                    .collect();
                for provider in core.providers.values() {
                    for model in &provider.models {
                        names.extend(
                            model
                                .exposed_names()
                                .into_iter()
                                .map(|name| format!("{}/{name}", provider.entity.name)),
                        );
                    }
                }
                use gproxy_protocol::openai::models as wire;
                let data = names
                    .into_iter()
                    .map(|id| {
                        wire::Model::builder(
                            id,
                            Some(0),
                            Some(wire::ModelObject::Model),
                            Some("gproxy".into()),
                        )
                        .build()
                    })
                    .collect();
                let body =
                    wire::ListModelsResponseBody::builder(data, wire::ListObject::List).build();
                return Ok(WireResponse {
                    status: http::StatusCode::OK,
                    headers: http::HeaderMap::new(),
                    body: HttpBody::Bytes(
                        serde_json::to_vec(&body).map_err(Error::invalid)?.into(),
                    ),
                });
            }
            let cancellation = CancellationToken::new();
            let guard = cancellation.clone().drop_guard();
            let mut call = self
                .engine
                .call(operation, request)
                .scope(&self.scope)
                .cancellation(cancellation);
            if let Some(model) = model {
                call = call.model(model);
            }
            let execution = call.send().await.map_err(|error| {
                Error::from_response(
                    error.status_code(),
                    &http::HeaderMap::new(),
                    error.to_string(),
                )
            })?;
            let (mut response, completion) = execution.into_parts();
            if let HttpBody::Stream(bytes) = response.body {
                response.body = HttpBody::Stream(Box::pin(stream::unfold(
                    (bytes, guard, completion),
                    |(mut bytes, guard, completion)| async move {
                        bytes
                            .next()
                            .await
                            .map(|chunk| (chunk, (bytes, guard, completion)))
                    },
                )));
            }
            Ok(response)
        })
    }
}
