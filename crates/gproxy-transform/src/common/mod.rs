//! Shared mechanical helpers. This module is not a wire-format IR.

pub(crate) const DEFAULT_CLAUDE_MAX_TOKENS: u64 = 16_384;

pub(crate) mod claude_message_controls;
pub(crate) mod content;
pub(crate) mod native;
pub(crate) mod responses;
pub(crate) mod stop;
pub(crate) mod stream;
pub(crate) mod tools;
pub(crate) mod usage;
