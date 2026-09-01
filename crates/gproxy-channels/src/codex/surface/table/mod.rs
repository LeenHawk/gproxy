mod aliases;
mod control;
mod services;

use std::sync::OnceLock;

use gproxy_channel_api::{
    ForwardRetry, ForwardSpec, SurfaceAction, SurfaceAffinity, SurfaceEntry, SurfaceTable,
    Synthesizer,
};
use gproxy_protocol::{PathPattern, Seg};
use http::Method;

pub(super) const GET: &Method = &Method::GET;
pub(super) const POST: &Method = &Method::POST;
pub(super) const PUT: &Method = &Method::PUT;
pub(super) const DELETE: &Method = &Method::DELETE;

pub(super) const SERVICE_PREFIXES: &[&[&str]] = &[
    &["api", "codex"],
    &["backend-api", "wham"],
    &["backend-api", "codex"],
    &["backend-api"],
    &["codex"],
];

pub(super) const PS_PREFIXES: &[&[&str]] = &[
    &["api", "codex", "ps"],
    &["backend-api", "wham", "ps"],
    &["backend-api", "codex", "ps"],
    &["backend-api", "ps"],
    &["codex", "ps"],
    &["ps"],
];

pub(super) const CURRENT_PREFIXES: &[&[&str]] = &[
    &[],
    &["backend-api"],
    &["api", "codex"],
    &["backend-api", "codex"],
    &["codex"],
];

pub(super) fn table() -> SurfaceTable {
    static ENTRIES: OnceLock<Vec<SurfaceEntry>> = OnceLock::new();
    SurfaceTable(ENTRIES.get_or_init(build).as_slice())
}

fn build() -> Vec<SurfaceEntry> {
    let mut entries = Vec::new();
    aliases::push(&mut entries);
    services::push(&mut entries);
    control::push(&mut entries);
    entries
}

pub(super) fn pattern(prefix: &[&'static str], tail: &[Seg]) -> PathPattern {
    let mut segments = prefix.iter().copied().map(Seg::Lit).collect::<Vec<_>>();
    segments.extend_from_slice(tail);
    PathPattern(Box::leak(segments.into_boxed_slice()))
}

pub(super) fn alias(
    method: &'static Method,
    pattern: PathPattern,
    canonical_path: &'static str,
) -> SurfaceEntry {
    SurfaceEntry {
        method,
        pattern,
        affinity: SurfaceAffinity::None,
        action: SurfaceAction::OperationAlias { canonical_path },
    }
}

pub(super) fn synth(
    method: &'static Method,
    pattern: PathPattern,
    affinity: SurfaceAffinity,
    handler: &'static dyn Synthesizer,
    upstream: bool,
) -> SurfaceEntry {
    SurfaceEntry {
        method,
        pattern,
        affinity,
        action: SurfaceAction::Synthesize { handler, upstream },
    }
}

pub(super) fn public_synth(
    method: &'static Method,
    pattern: PathPattern,
    handler: &'static dyn Synthesizer,
) -> SurfaceEntry {
    SurfaceEntry {
        method,
        pattern,
        affinity: SurfaceAffinity::None,
        action: SurfaceAction::PublicSynthesize { handler },
    }
}

pub(super) fn forward(
    method: &'static Method,
    pattern: PathPattern,
    affinity: SurfaceAffinity,
    label: &'static str,
    upstream_template: &'static str,
    retry: ForwardRetry,
    websocket: bool,
) -> SurfaceEntry {
    let spec = ForwardSpec {
        label,
        upstream_template,
        retry,
    };
    SurfaceEntry {
        method,
        pattern,
        affinity,
        action: if websocket {
            SurfaceAction::ForwardWebSocket(spec)
        } else {
            SurfaceAction::Forward(spec)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_channel_api::ForwardRetry;
    use gproxy_protocol::match_path;

    fn action(method: &Method, path: &str) -> Option<(&'static SurfaceAction, SurfaceAffinity)> {
        table()
            .0
            .iter()
            .find(|entry| entry.method == method && match_path(entry.pattern, path).is_some())
            .map(|entry| (&entry.action, entry.affinity))
    }

    #[test]
    fn aliases_and_affinity_rows_are_inspectable() {
        assert!(matches!(
            action(&Method::POST, "/backend-api/wham/responses")
                .unwrap()
                .0,
            SurfaceAction::OperationAlias {
                canonical_path: "/v1/responses"
            }
        ));
        for (method, path) in [
            (&Method::GET, "/api/codex/models"),
            (&Method::GET, "/plugins/featured"),
            (&Method::POST, "/api/codex/ps/plugins/list"),
        ] {
            assert!(matches!(
                action(method, path).unwrap().0,
                SurfaceAction::Forward(ForwardSpec {
                    retry: ForwardRetry::Retryable,
                    ..
                })
            ));
        }
        for (method, path) in [
            (&Method::POST, "/api/codex/agent-identities/me"),
            (&Method::POST, "/api/codex/ps/plugins/installed"),
            (&Method::POST, "/plugins/plugin-1/enable"),
        ] {
            assert!(matches!(
                action(method, path).unwrap().0,
                SurfaceAction::Forward(ForwardSpec {
                    retry: ForwardRetry::SingleAttempt,
                    ..
                })
            ));
        }
        assert!(table().0.iter().all(|entry| !matches!(
            &entry.action,
            SurfaceAction::ForwardWebSocket(ForwardSpec {
                retry: ForwardRetry::Retryable,
                ..
            })
        )));
        for path in [
            "/v1/memories/trace_summarize",
            "/api/codex/memories/trace_summarize",
            "/backend-api/wham/memories/trace_summarize",
            "/backend-api/codex/memories/trace_summarize",
            "/backend-api/memories/trace_summarize",
            "/codex/memories/trace_summarize",
        ] {
            assert!(matches!(
                action(&Method::POST, path).unwrap().0,
                SurfaceAction::OperationAlias {
                    canonical_path: "/v1/memories/trace_summarize"
                }
            ));
        }
        assert!(matches!(
            action(&Method::POST, "/api/codex/ps/mcp").unwrap().1,
            SurfaceAffinity::Header {
                name: "mcp-session-id",
                ..
            }
        ));
        assert!(matches!(
            action(&Method::GET, "/ps/plugins/list").unwrap().1,
            SurfaceAffinity::None
        ));
        assert!(matches!(
            action(&Method::GET, "/ps/plugins/plugin-1").unwrap().1,
            SurfaceAffinity::PathParam {
                name: "plugin_id",
                ..
            }
        ));
        assert!(matches!(
            action(&Method::GET, "/api/codex/remote/control").unwrap().1,
            SurfaceAffinity::HeaderOrBodyField {
                header: "x-codex-server-id",
                body_field: "server_id",
                ..
            }
        ));
        for (method, path) in [
            (&Method::GET, "/plugins/featured"),
            (&Method::GET, "/backend-api/plugins/featured"),
            (&Method::POST, "/plugins/plugin-1/enable"),
            (
                &Method::POST,
                "/backend-api/public/plugins/workspace/upload-url",
            ),
            (&Method::PUT, "/ps/plugins/plugin-1/shares"),
        ] {
            assert!(matches!(
                action(method, path).unwrap().0,
                SurfaceAction::Forward(_)
            ));
        }
        assert!(matches!(
            action(&Method::GET, "/backend-api/accounts/account-1/settings")
                .unwrap()
                .0,
            SurfaceAction::Synthesize { .. }
        ));
        assert!(matches!(
            action(
                &Method::POST,
                "/backend-api/wham/remote/control/server/refresh"
            )
            .unwrap()
            .1,
            SurfaceAffinity::ResponseBodyToken {
                request_body_field: Some("server_id"),
                ..
            }
        ));
        assert!(matches!(
            action(
                &Method::POST,
                "/backend-api/wham/remote/control/server/pair/status"
            )
            .unwrap()
            .1,
            SurfaceAffinity::BearerToken { .. }
        ));
        assert!(matches!(
            action(
                &Method::GET,
                "/backend-api/wham/remote/control/environments/env-1/clients"
            )
            .unwrap()
            .1,
            SurfaceAffinity::PathParam {
                name: "environment_id",
                ..
            }
        ));
    }
}
