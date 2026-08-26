use gproxy_channel_api::{SurfaceAffinity, SurfaceEntry};
use gproxy_protocol::Seg;

use super::super::super::remote;
use super::super::{DELETE, GET, POST, forward, pattern, synth};
use super::{RETRYABLE, SINGLE};

pub(super) fn push(entries: &mut Vec<SurfaceEntry>, prefix: &[&'static str]) {
    entries.push(forward(
        GET,
        pattern(prefix, &[Seg::Lit("plugins"), Seg::Lit("featured")]),
        SurfaceAffinity::None,
        "plugins",
        "/plugins/featured",
        RETRYABLE,
        false,
    ));
    for (action, upstream) in [
        ("enable", "/plugins/{plugin_id}/enable"),
        ("uninstall", "/plugins/{plugin_id}/uninstall"),
    ] {
        entries.push(forward(
            POST,
            pattern(
                prefix,
                &[
                    Seg::Lit("plugins"),
                    Seg::Param("plugin_id"),
                    Seg::Lit(action),
                ],
            ),
            remote::PLUGIN,
            "plugins",
            upstream,
            SINGLE,
            false,
        ));
    }
    entries.push(synth(
        GET,
        pattern(
            prefix,
            &[
                Seg::Lit("accounts"),
                Seg::Param("account_id"),
                Seg::Lit("settings"),
            ],
        ),
        SurfaceAffinity::None,
        &super::super::super::local::HANDLER,
        false,
    ));
    entries.push(forward(
        POST,
        pattern(
            prefix,
            &[
                Seg::Lit("public"),
                Seg::Lit("plugins"),
                Seg::Lit("workspace"),
                Seg::Lit("upload-url"),
            ],
        ),
        SurfaceAffinity::None,
        "plugins",
        "/public/plugins/workspace/upload-url",
        SINGLE,
        false,
    ));
    for method in [POST, DELETE] {
        entries.push(forward(
            method,
            pattern(
                prefix,
                &[
                    Seg::Lit("public"),
                    Seg::Lit("plugins"),
                    Seg::Lit("workspace"),
                    Seg::Param("plugin_id"),
                ],
            ),
            remote::PLUGIN,
            "plugins",
            "/public/plugins/workspace/{plugin_id}",
            SINGLE,
            false,
        ));
    }
    entries.push(forward(
        POST,
        pattern(
            prefix,
            &[
                Seg::Lit("public"),
                Seg::Lit("plugins"),
                Seg::Lit("workspace"),
            ],
        ),
        SurfaceAffinity::None,
        "plugins",
        "/public/plugins/workspace",
        SINGLE,
        false,
    ));
}
