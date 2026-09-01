use gproxy_channel_api::SurfaceEntry;
use gproxy_protocol::Seg;

use super::{GET, POST, SERVICE_PREFIXES, alias, pattern};

pub(super) fn push(entries: &mut Vec<SurfaceEntry>) {
    for prefix in SERVICE_PREFIXES {
        entries.push(alias(
            GET,
            pattern(prefix, &[Seg::Lit("responses")]),
            "/v1/responses",
        ));
        for (tail, canonical) in [
            (
                &[Seg::Lit("memories"), Seg::Lit("trace_summarize")][..],
                "/v1/memories/trace_summarize",
            ),
            (&[Seg::Lit("responses")][..], "/v1/responses"),
            (&[Seg::Lit("guardian")][..], "/v1/guardian"),
            (
                &[Seg::Lit("guardian-classifier")][..],
                "/v1/guardian-classifier",
            ),
            (
                &[Seg::Lit("responses"), Seg::Lit("compact")][..],
                "/v1/responses/compact",
            ),
            (
                &[Seg::Lit("images"), Seg::Lit("generations")][..],
                "/v1/images/generations",
            ),
            (
                &[Seg::Lit("images"), Seg::Lit("edits")][..],
                "/v1/images/edits",
            ),
            (
                &[Seg::Lit("alpha"), Seg::Lit("search")][..],
                "/v1/alpha/search",
            ),
            (
                &[Seg::Lit("realtime"), Seg::Lit("calls")][..],
                "/v1/realtime/calls",
            ),
        ] {
            entries.push(alias(POST, pattern(prefix, tail), canonical));
        }
    }
}
