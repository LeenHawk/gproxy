use gproxy_channel_api::{SurfaceAffinity, SurfaceEntry};
use gproxy_protocol::Seg;

use super::super::remote;
use super::{
    CURRENT_PREFIXES, DELETE, GET, POST, PS_PREFIXES, PUT, SERVICE_PREFIXES, forward, pattern,
    synth,
};

pub(super) fn push(entries: &mut Vec<SurfaceEntry>) {
    for prefix in CURRENT_PREFIXES {
        current_client_rows(entries, prefix);
    }
    for prefix in SERVICE_PREFIXES {
        entries.push(forward(
            GET,
            pattern(prefix, &[Seg::Lit("models")]),
            SurfaceAffinity::None,
            "codex_models",
            "/codex/models",
            false,
        ));
        entries.push(forward(
            GET,
            pattern(prefix, &[Seg::Lit("models"), Seg::Rest("model_rest")]),
            SurfaceAffinity::None,
            "codex_models",
            "/codex/models/{model_rest}",
            false,
        ));
        for method in [GET, POST] {
            entries.push(forward(
                method,
                pattern(
                    prefix,
                    &[Seg::Lit("agent-identities"), Seg::Rest("agent_rest")],
                ),
                SurfaceAffinity::None,
                "agent_identity",
                "/wham/agent-identities/{agent_rest}",
                false,
            ));
        }
        remote_rows(entries, prefix);
    }
    for prefix in PS_PREFIXES {
        entries.push(forward(
            POST,
            pattern(prefix, &[Seg::Lit("mcp")]),
            remote::MCP,
            "ps_mcp",
            "/ps/mcp",
            false,
        ));
        plugin_rows(entries, prefix, "plugins");
        plugin_rows(entries, prefix, "apps");
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
            false,
        ));
    }
}

fn current_client_rows(entries: &mut Vec<SurfaceEntry>, prefix: &[&'static str]) {
    entries.push(forward(
        GET,
        pattern(prefix, &[Seg::Lit("plugins"), Seg::Lit("featured")]),
        SurfaceAffinity::None,
        "plugins",
        "/plugins/featured",
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
        &super::super::local::HANDLER,
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
        false,
    ));
}

fn remote_rows(entries: &mut Vec<SurfaceEntry>, prefix: &[&'static str]) {
    entries.push(forward(
        POST,
        pattern(prefix, &[Seg::Lit("remote"), Seg::Lit("control")]),
        remote::REMOTE_CREATE,
        "remote_control",
        "/wham/remote/control",
        false,
    ));
    for method in [GET, DELETE] {
        entries.push(forward(
            method,
            pattern(prefix, &[Seg::Lit("remote"), Seg::Lit("control")]),
            remote::REMOTE_HTTP,
            "remote_control",
            "/wham/remote/control",
            false,
        ));
    }
    entries.push(forward(
        POST,
        pattern(
            prefix,
            &[Seg::Lit("remote"), Seg::Lit("control"), Seg::Lit("server")],
        ),
        remote::REMOTE_CREATE,
        "remote_control",
        "/wham/remote/control/server",
        false,
    ));
    for (tail, upstream, affinity, label) in [
        (
            "enroll",
            "/wham/remote/control/server/enroll",
            remote::REMOTE_CREATE,
            "remote_control",
        ),
        (
            "refresh",
            "/wham/remote/control/server/refresh",
            remote::REMOTE_REFRESH,
            "remote_control",
        ),
        (
            "pair",
            "/wham/remote/control/server/pair",
            remote::REMOTE_SOCKET,
            "remote_control_token",
        ),
    ] {
        entries.push(forward(
            POST,
            pattern(
                prefix,
                &[
                    Seg::Lit("remote"),
                    Seg::Lit("control"),
                    Seg::Lit("server"),
                    Seg::Lit(tail),
                ],
            ),
            affinity,
            label,
            upstream,
            false,
        ));
    }
    entries.push(forward(
        POST,
        pattern(
            prefix,
            &[
                Seg::Lit("remote"),
                Seg::Lit("control"),
                Seg::Lit("server"),
                Seg::Lit("pair"),
                Seg::Lit("status"),
            ],
        ),
        remote::REMOTE_SOCKET,
        "remote_control_token",
        "/wham/remote/control/server/pair/status",
        false,
    ));
    entries.push(forward(
        GET,
        pattern(
            prefix,
            &[
                Seg::Lit("remote"),
                Seg::Lit("control"),
                Seg::Lit("environments"),
                Seg::Param("environment_id"),
                Seg::Lit("clients"),
            ],
        ),
        remote::REMOTE_ENVIRONMENT,
        "remote_control",
        "/wham/remote/control/environments/{environment_id}/clients",
        false,
    ));
    entries.push(forward(
        DELETE,
        pattern(
            prefix,
            &[
                Seg::Lit("remote"),
                Seg::Lit("control"),
                Seg::Lit("environments"),
                Seg::Param("environment_id"),
                Seg::Lit("clients"),
                Seg::Param("client_id"),
            ],
        ),
        remote::REMOTE_ENVIRONMENT,
        "remote_control",
        "/wham/remote/control/environments/{environment_id}/clients/{client_id}",
        false,
    ));
    entries.push(synth(
        GET,
        pattern(
            prefix,
            &[Seg::Lit("remote"), Seg::Lit("control"), Seg::Lit("server")],
        ),
        SurfaceAffinity::None,
        &super::super::local::HANDLER,
        false,
    ));
    entries.push(forward(
        GET,
        pattern(
            prefix,
            &[Seg::Lit("remote"), Seg::Lit("control"), Seg::Lit("server")],
        ),
        remote::REMOTE_SOCKET,
        "remote_control_ws",
        "/wham/remote/control/server",
        true,
    ));
    for method in [GET, POST, DELETE] {
        entries.push(forward(
            method,
            pattern(
                prefix,
                &[
                    Seg::Lit("remote"),
                    Seg::Lit("control"),
                    Seg::Rest("remote_rest"),
                ],
            ),
            remote::REMOTE_HTTP,
            "remote_control",
            "/wham/remote/control/{remote_rest}",
            false,
        ));
    }
}

fn plugin_rows(entries: &mut Vec<SurfaceEntry>, prefix: &[&'static str], collection: &'static str) {
    for (word, upstream) in collection_rows(collection) {
        for method in [GET, POST, DELETE] {
            entries.push(forward(
                method,
                pattern(prefix, &[Seg::Lit(collection), Seg::Lit(word)]),
                SurfaceAffinity::None,
                "plugins",
                upstream,
                false,
            ));
        }
    }
    let (base, nested) = if collection == "plugins" {
        (
            "/ps/plugins/{plugin_id}",
            "/ps/plugins/{plugin_id}/{plugin_rest}",
        )
    } else {
        ("/ps/apps/{plugin_id}", "/ps/apps/{plugin_id}/{plugin_rest}")
    };
    for method in [GET, POST, DELETE] {
        entries.push(forward(
            method,
            pattern(prefix, &[Seg::Lit(collection), Seg::Param("plugin_id")]),
            remote::PLUGIN,
            "plugins",
            base,
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
            false,
        ));
    }
}

fn collection_rows(collection: &str) -> [(&'static str, &'static str); 5] {
    if collection == "plugins" {
        [
            ("list", "/ps/plugins/list"),
            ("installed", "/ps/plugins/installed"),
            ("search", "/ps/plugins/search"),
            ("suggested", "/ps/plugins/suggested"),
            ("workspace", "/ps/plugins/workspace"),
        ]
    } else {
        [
            ("list", "/ps/apps/list"),
            ("installed", "/ps/apps/installed"),
            ("search", "/ps/apps/search"),
            ("suggested", "/ps/apps/suggested"),
            ("workspace", "/ps/apps/workspace"),
        ]
    }
}
