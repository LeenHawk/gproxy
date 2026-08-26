use gproxy_channel_api::{ForwardRetry, SurfaceAffinity, SurfaceEntry};
use gproxy_protocol::Seg;

use super::super::super::remote;
use super::super::{DELETE, GET, POST, forward, pattern};
use super::{RETRYABLE, SINGLE};

pub(super) fn push(
    entries: &mut Vec<SurfaceEntry>,
    prefix: &[&'static str],
    collection: &'static str,
) {
    for (word, upstream, post_retry) in collection_rows(collection) {
        for (method, retry) in [(GET, RETRYABLE), (DELETE, SINGLE)] {
            entries.push(forward(
                method,
                pattern(prefix, &[Seg::Lit(collection), Seg::Lit(word)]),
                SurfaceAffinity::None,
                "plugins",
                upstream,
                retry,
                false,
            ));
        }
        entries.push(forward(
            POST,
            pattern(prefix, &[Seg::Lit(collection), Seg::Lit(word)]),
            SurfaceAffinity::None,
            "plugins",
            upstream,
            post_retry,
            false,
        ));
    }
    let (base, nested) = if collection == "plugins" {
        (
            "/ps/plugins/{plugin_id}",
            "/ps/plugins/{plugin_id}/{plugin_rest}",
        )
    } else {
        ("/ps/apps/{plugin_id}", "/ps/apps/{plugin_id}/{plugin_rest}")
    };
    for (method, retry) in [(GET, RETRYABLE), (POST, SINGLE), (DELETE, SINGLE)] {
        entries.push(forward(
            method,
            pattern(prefix, &[Seg::Lit(collection), Seg::Param("plugin_id")]),
            remote::PLUGIN,
            "plugins",
            base,
            retry,
            false,
        ));
        entries.push(forward(
            method,
            pattern(
                prefix,
                &[
                    Seg::Lit(collection),
                    Seg::Param("plugin_id"),
                    Seg::Rest("plugin_rest"),
                ],
            ),
            remote::PLUGIN,
            "plugins",
            nested,
            retry,
            false,
        ));
    }
}

fn collection_rows(collection: &str) -> [(&'static str, &'static str, ForwardRetry); 5] {
    if collection == "plugins" {
        [
            ("list", "/ps/plugins/list", RETRYABLE),
            ("installed", "/ps/plugins/installed", SINGLE),
            ("search", "/ps/plugins/search", SINGLE),
            ("suggested", "/ps/plugins/suggested", SINGLE),
            ("workspace", "/ps/plugins/workspace", SINGLE),
        ]
    } else {
        [
            ("list", "/ps/apps/list", RETRYABLE),
            ("installed", "/ps/apps/installed", SINGLE),
            ("search", "/ps/apps/search", SINGLE),
            ("suggested", "/ps/apps/suggested", SINGLE),
            ("workspace", "/ps/apps/workspace", SINGLE),
        ]
    }
}
