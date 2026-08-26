mod current;
mod plugins;
mod remote_rows;

use gproxy_channel_api::{ForwardRetry, SurfaceAffinity, SurfaceEntry};
use gproxy_protocol::Seg;

use super::super::remote;
use super::{CURRENT_PREFIXES, GET, POST, PS_PREFIXES, PUT, SERVICE_PREFIXES, forward, pattern};

pub(super) const RETRYABLE: ForwardRetry = ForwardRetry::Retryable;
pub(super) const SINGLE: ForwardRetry = ForwardRetry::SingleAttempt;

pub(super) fn push(entries: &mut Vec<SurfaceEntry>) {
    for prefix in CURRENT_PREFIXES {
        current::push(entries, prefix);
    }
    for prefix in SERVICE_PREFIXES {
        entries.push(forward(
            GET,
            pattern(prefix, &[Seg::Lit("models")]),
            SurfaceAffinity::None,
            "codex_models",
            "/codex/models",
            RETRYABLE,
            false,
        ));
        entries.push(forward(
            GET,
            pattern(prefix, &[Seg::Lit("models"), Seg::Rest("model_rest")]),
            SurfaceAffinity::None,
            "codex_models",
            "/codex/models/{model_rest}",
            RETRYABLE,
            false,
        ));
        for (method, retry) in [(GET, RETRYABLE), (POST, SINGLE)] {
            entries.push(forward(
                method,
                pattern(
                    prefix,
                    &[Seg::Lit("agent-identities"), Seg::Rest("agent_rest")],
                ),
                SurfaceAffinity::None,
                "agent_identity",
                "/wham/agent-identities/{agent_rest}",
                retry,
                false,
            ));
        }
        remote_rows::push(entries, prefix);
    }
    for prefix in PS_PREFIXES {
        entries.push(forward(
            POST,
            pattern(prefix, &[Seg::Lit("mcp")]),
            remote::MCP,
            "ps_mcp",
            "/ps/mcp",
            SINGLE,
            false,
        ));
        plugins::push(entries, prefix, "plugins");
        plugins::push(entries, prefix, "apps");
        entries.push(forward(
            PUT,
            pattern(
                prefix,
                &[
                    Seg::Lit("plugins"),
                    Seg::Param("plugin_id"),
                    Seg::Lit("shares"),
                ],
            ),
            remote::PLUGIN,
            "plugins",
            "/ps/plugins/{plugin_id}/shares",
            SINGLE,
            false,
        ));
    }
}
