//! Per-call metering. Reading usage does not query account quotas or settle a bill.
//!
//! A channel does not read usage. Its job on the response side is to shape
//! whatever its upstream answered into the standard response of the
//! operation, and the host reads usage from that standard response with
//! `gproxy_protocol::usage`, by operation and dialect. What a channel may add
//! is what its vendor reports beside the standard usage object — a price it
//! charged, a cache counter under a name of its own, the model that actually
//! served — through [`UsageExtras`].

use gproxy_protocol::OperationKey;
use http::HeaderMap;
use serde_json::Value;

// The metering vocabulary is the protocol's, where the standard readers
// produce it; it stays reachable at this path for every existing user.
pub use gproxy_protocol::usage::{
    NormalizedUsage, ResponseUsage, TokenUsage, UsageAttempt, UsageCompleteness, UsageStreamEnd,
    UsageTransport,
};

/// Where a vendor's own usage fields are read from.
pub struct UsageSource<'a> {
    /// The operation served, in the dialect of the response the channel
    /// returned.
    pub operation: OperationKey,
    /// The headers of the response the channel returned.
    pub headers: &'a HeaderMap,
    /// The JSON that carried the standard usage: the whole body of a
    /// buffered reply, or the latest event of a stream or socket that
    /// carried usage. `Null` when the response carried none.
    pub root: &'a Value,
}

/// A vendor's fields beside the standard usage object.
///
/// The standard reading has already filled `usage` when this runs, from the
/// same response; an implementation adds what only its vendor reports and
/// may correct a standard field the vendor names differently. When the
/// response carried no standard usage, `usage` starts empty and is kept only
/// if this adds something. Metric and dimension keys are part of the pricing
/// contract: an operator's rate rows name them.
pub trait UsageExtras: Send + Sync {
    fn read(&self, source: UsageSource<'_>, usage: &mut NormalizedUsage);
}

/// The standard reading of a response with the channel's extras applied
/// over it: what a response settles with. `standard` is `None` when the
/// response carried no standard usage; the extras may still find something
/// of their own, and the reading stays `None` only when they do not.
pub fn with_extras(
    extras: Option<&dyn UsageExtras>,
    source: UsageSource<'_>,
    standard: Option<NormalizedUsage>,
) -> Option<NormalizedUsage> {
    let Some(extras) = extras else {
        return standard;
    };
    let found = standard.is_some();
    let mut usage = standard.unwrap_or_default();
    extras.read(source, &mut usage);
    (found || usage != NormalizedUsage::default()).then_some(usage)
}

/// The usage object inside a standard body or event: at the root of a Chat
/// or Responses body, a Chat chunk or a Claude `message_delta`, under
/// `response` in a Responses event, under `message` in a Claude
/// `message_start`.
pub fn usage_object(root: &Value) -> Option<&Value> {
    ["/usage", "/response/usage", "/message/usage"]
        .into_iter()
        .filter_map(|pointer| root.pointer(pointer))
        .find(|usage| usage.is_object())
}
