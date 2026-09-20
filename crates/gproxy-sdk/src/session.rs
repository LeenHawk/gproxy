//! Reading a conversation identity out of a request.
//!
//! The ladder is the gateway header first, then the dialect's own session
//! field in the body, then a fingerprint of what the request does carry.
//! Filled in by the call phase.
