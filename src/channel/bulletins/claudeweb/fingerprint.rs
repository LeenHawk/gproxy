//! Browser TLS/HTTP fingerprint for claude.ai's Cloudflare-fronted web API.

use wreq::IntoEmulation;

pub(super) fn default_emulation() -> wreq::Emulation {
    wreq_util::Emulation::Edge148.into_emulation()
}
