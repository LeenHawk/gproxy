//! The tier AI Studio served, which it names in a response header.
//!
//! Everything else AI Studio reports is standard: `usageMetadata` on the
//! native surface and OpenAI's shapes on the compatibility surface, both
//! read by the host from the response as the channel returns it.

use super::Aistudio;
use crate::channel::{NormalizedUsage, UsageExtras, UsageSource};
use gproxy_protocol::WireFamily;

/// The response header AI Studio uses to name the tier it actually served.
const SERVICE_TIER: &str = "x-gemini-service-tier";

impl UsageExtras for Aistudio {
    /// A native reply that reported usage is qualified by the tier the header
    /// names. The header alone reports no consumption, so a reply without
    /// usage stays without.
    fn read(&self, source: UsageSource<'_>, usage: &mut NormalizedUsage) {
        if source.operation.dialect.family() != WireFamily::Gemini
            || *usage == NormalizedUsage::default()
        {
            return;
        }
        if let Some(tier) = source
            .headers
            .get(SERVICE_TIER)
            .and_then(|value| value.to_str().ok())
        {
            usage.dimensions.insert("service_tier".into(), tier.into());
            usage.actual_service_tier = Some(tier.to_owned());
        }
    }
}
