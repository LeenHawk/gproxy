//! The host binds one direct Interactions generation edge to an attempt.
use super::{Call, Converted};
use gproxy_protocol::{
    HttpBody, Operation, OperationKey,
    adapt::interactions::{self, InteractionBinding, InteractionFacts, InteractionOutcome},
    connection::Bytes,
    transform::TransformError,
};
use gproxy_seaorm::BatchConnectionTrait;

pub(crate) async fn run<C: BatchConnectionTrait + Send + Sync + 'static>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let operation = if call.collect {
        Operation::StreamGenerateContent
    } else if call.synthesize {
        Operation::GenerateContent
    } else {
        call.client.operation
    };
    let native = OperationKey {
        operation,
        dialect: call.target,
    };
    let model = call.model()?;
    let endpoint = super::generate_endpoint(
        call.target,
        model,
        operation == Operation::StreamGenerateContent,
    )?;
    let max_tokens = call
        .upstream
        .attempt()
        .request
        .target
        .provider
        .models
        .iter()
        .find(|row| row.upstream_name == model)
        .and_then(|row| row.metadata["max_output_tokens"].as_u64())
        .unwrap_or(8192);
    let request = gproxy_protocol::WireRequest {
        method: http::Method::POST,
        path: call.request.path.to_owned(),
        query: None,
        headers: call.request.headers.clone(),
        body: serde_json::from_slice(call.body())?,
    };
    match interactions::invoke(
        call.upstream,
        &native,
        request,
        InteractionBinding {
            client: call.client,
            native,
            endpoint,
            facts: InteractionFacts {
                id: crate::ids::random_id(),
                model: model.to_owned(),
                created_at: call.now_ms.div_euclid(1000),
                max_tokens,
                namespace: super::generate::namespace(),
            },
        },
        call.limits,
    )
    .await?
    {
        InteractionOutcome::Success { response, report } => {
            for diagnostic in report.diagnostics {
                tracing::debug!(
                    field = diagnostic.field,
                    message = diagnostic.message,
                    "Interactions conversion"
                );
            }
            Ok(Converted::Success(response))
        }
        InteractionOutcome::Rejected(mut response) => {
            // A rejected upstream remains a response, in the client's error dialect.
            let body = std::mem::replace(&mut response.body, HttpBody::Bytes(Bytes::new()));
            let bytes = gproxy_protocol::codec::read_http_body(body, call.limits)
                .await
                .map_err(|e| TransformError::invalid_result("interaction.error", e.to_string()))?;
            if let Ok(value) = serde_json::from_slice(&bytes) {
                let detail = super::upstream_error::unwrap(value);
                if let Some(value) = super::upstream_error::body(
                    &detail,
                    response.status.as_u16(),
                    call.client.dialect,
                ) {
                    response.headers.remove(http::header::CONTENT_LENGTH);
                    response.headers.remove(http::header::CONTENT_ENCODING);
                    response.headers.insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/json"),
                    );
                    response.body = HttpBody::Bytes(value.to_string().into());
                    return Ok(Converted::Rejected(response));
                }
            }
            response.body = HttpBody::Bytes(bytes);
            Ok(Converted::Rejected(response))
        }
    }
}
