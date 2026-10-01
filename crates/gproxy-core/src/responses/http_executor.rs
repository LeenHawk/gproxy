//! HTTP segments share their turn's observation and settlement funnel. The
//! SDK supplies the admitted provider walk; core owns each credential retry.

use super::*;
use gproxy_protocol::{
    HttpBody, Operation, OperationKey, WireResponse, capability::CapabilityFuture,
};

pub trait ResponsesHttpExecutor: gproxy_client::ClientBounds {
    fn send<'a>(
        &'a self,
        session: &'a ResponsesSession,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, CoreResult<WireResponse<HttpBody>>>;
}

impl ResponsesSession {
    /// Once a segment has been accepted, subsequent segments must keep its
    /// account and model, including when they are rejected before streaming.
    pub fn committed_http_binding(&self) -> Option<ResponsesBinding> {
        self.current()
            .filter(|turn| turn.http_committed.load(Ordering::Relaxed))
            .and_then(|_| self.binding())
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Core<C> {
    pub async fn send_responses_http(
        &self,
        session: &ResponsesSession,
        context: Arc<RequestContext>,
        wire: WireRequest<HttpBody>,
    ) -> CoreResult<WireResponse<HttpBody>> {
        let turn = session
            .current()
            .ok_or_else(|| CoreError::InvalidTarget("Responses turn is no longer active".into()))?;
        if turn.attempt.request.request_id != context.request_id {
            return Err(CoreError::InvalidTarget(
                "Responses HTTP plan belongs to another turn".into(),
            ));
        }
        let mut context = (*context).clone();
        context.operation = OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAi,
        };
        if let Some(binding) = session.committed_http_binding() {
            if context.target.provider.entity.id != binding.provider_id
                || context.target.upstream_model.as_deref() != Some(&binding.model)
            {
                return Err(CoreError::InvalidTarget(
                    "Responses segment must keep its accepted target".into(),
                ));
            }
            context
                .target
                .credentials
                .retain(|credential| credential.id == binding.credential_id);
        }
        let context = Arc::new(context);
        if context.snapshot.observation.settlement {
            let pending = turn.funnel.pending_cost().await;
            crate::budget::check_pending(self.store(), &context, now_ms(), pending.as_ref())
                .await?;
        }
        // An ambiguous transport failure may have executed the POST already.
        // Only explicit HTTP rejections participate in the retry loop.
        let answer =
            crate::execute::run_http_attempts(self, context, wire, turn.funnel.clone(), false)
                .await?;
        if answer.response.status.is_success()
            && let Some(attempt) = answer.attempt
        {
            let dialect = match crate::convert::route_for_model(
                &attempt.request.target.provider,
                attempt.request.operation,
                attempt.request.target.upstream_model.as_deref(),
            )? {
                crate::convert::Route::TransformTo { target }
                    if target.dialect != Dialect::OpenAiResponsesWebSocket =>
                {
                    target.dialect
                }
                _ => Dialect::OpenAi,
            };
            turn.funnel.settle_on(&attempt.request.target);
            session.bind(
                attempt.request.clone(),
                attempt.credential.clone(),
                dialect,
                None,
                attempt.agent_assignment.clone(),
            );
            turn.http_committed.store(true, Ordering::Relaxed);
        }
        Ok(answer.response)
    }
}
