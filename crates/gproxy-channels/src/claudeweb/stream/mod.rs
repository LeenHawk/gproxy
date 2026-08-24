mod codec;
mod modern;
mod sse;

use serde::{Deserialize, Serialize};

pub(super) use codec::Codec;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct SessionState {
    pub conversation: String,
    pub model: String,
    pub message_id: String,
    pub input_tokens: u64,
}
