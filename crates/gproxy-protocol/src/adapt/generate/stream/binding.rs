use super::super::GenerationStateAccess;
use crate::{
    capability::StateStore, transform::TransformError, transform::identity::IdentityTarget,
};
use std::time::SystemTime;

/// The state an invocation or a WebSocket connection was prepared against.
///
/// This used to be reserved through the state store: the full original and
/// prepared requests were CAS'd twice per stream and read back at its start
/// and end, and a connection CAS'd a marker of its own. Nothing read those
/// records on a later request and the host never replays, so they only kept a
/// started invocation from sending again or moving to other state, which the
/// invocation can enforce itself. The binding therefore lives in memory. A
/// store and scope carry no identity that can be compared without I/O, so a
/// step is checked against the target, conversation and expiry it was
/// prepared with; those are what every record it writes is keyed and aged by.
#[derive(Clone)]
pub(super) struct StateBinding {
    target: IdentityTarget,
    conversation_key: String,
    expires_at: SystemTime,
}

impl StateBinding {
    pub fn new<S: StateStore>(state: &GenerationStateAccess<'_, S>) -> Self {
        Self {
            target: state.target.clone(),
            conversation_key: state.conversation_key.clone(),
            expires_at: state.expires_at,
        }
    }
    /// Refuses a step whose state differs from the one this was prepared with.
    pub fn check<S: StateStore>(
        &self,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<(), TransformError> {
        if state.target != self.target
            || state.conversation_key != self.conversation_key
            || state.expires_at != self.expires_at
        {
            return Err(super::conflict(
                "invocation is bound to another state authority",
            ));
        }
        Ok(())
    }
    /// A connection cache outlives the invocation that bound it, and later
    /// invocations carry their own expiry, so only the conversation must match
    /// and the first binding must not have expired.
    pub fn serves<S: StateStore>(&self, state: &GenerationStateAccess<'_, S>) -> bool {
        state.conversation_key == self.conversation_key && state.now < self.expires_at
    }
}
