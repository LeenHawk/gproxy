//! Pair preparation reuses the declared buffered request mappings and scoped
//! history recovery, then explicitly selects native streaming transport.

mod chat_claude;
mod chat_gemini;
mod chat_responses;
mod claude_gemini;
mod claude_responses;
mod gemini_responses;
mod limits;

use super::{StreamInvocation, StreamSettings, StreamTarget, bridge::StreamBridge};
use crate::{
    adapt::generate::{
        GenerationStateAccess,
        prepared::{PreparedGeneration, RequestMode},
    },
    capability::StateStore,
    transform::TransformError,
};

pub use chat_gemini::ChatViaGeminiStreamFacts;
pub use claude_gemini::{ClaudeViaGeminiStreamFacts, GeminiViaClaudeStreamFacts};
pub use claude_responses::ResponsesViaClaudeStreamFacts;
pub use gemini_responses::ResponsesViaGeminiStreamFacts;

/// The tail every pair shares: adopt the prepared identities, let the pair
/// build its bridge, and start the invocation on the streaming target request.
async fn start<P, B, S>(
    original: P::Source,
    prepared: P,
    mut target: StreamTarget,
    settings: StreamSettings,
    state: &GenerationStateAccess<'_, S>,
    bridge: impl FnOnce(&P, &mut StreamTarget, &StreamSettings) -> Result<B, TransformError>,
) -> Result<StreamInvocation<B>, TransformError>
where
    P: PreparedGeneration,
    B: StreamBridge<ClientRequest = P::Source, NativeRequest = P::Target>,
    S: StateStore,
{
    target.identities = prepared.identities().clone();
    let bridge = bridge(&prepared, &mut target, &settings)?;
    let emit_usage = original.emit_usage();
    let mut invocation = StreamInvocation::new(
        original,
        prepared.target_request().clone().streaming(),
        target,
        settings,
        bridge,
        prepared.report().clone(),
        state,
    )
    .await?;
    invocation.emit_usage = emit_usage;
    Ok(invocation)
}
