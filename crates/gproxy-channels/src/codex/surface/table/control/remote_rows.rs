use gproxy_channel_api::{SurfaceAffinity, SurfaceEntry};
use gproxy_protocol::Seg;

use super::super::super::remote;
use super::super::{DELETE, GET, POST, forward, pattern, synth};
use super::{RETRYABLE, SINGLE};

pub(super) fn push(entries: &mut Vec<SurfaceEntry>, prefix: &[&'static str]) {
    entries.push(forward(
        POST,
        pattern(prefix, &[Seg::Lit("remote"), Seg::Lit("control")]),
        remote::REMOTE_CREATE,
        "remote_control",
        "/wham/remote/control",
        SINGLE,
        false,
    ));
    for (method, retry) in [(GET, RETRYABLE), (DELETE, SINGLE)] {
        entries.push(forward(
            method,
            pattern(prefix, &[Seg::Lit("remote"), Seg::Lit("control")]),
            remote::REMOTE_HTTP,
            "remote_control",
            "/wham/remote/control",
            retry,
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
        SINGLE,
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
            SINGLE,
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
        SINGLE,
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
        RETRYABLE,
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
        SINGLE,
        false,
    ));
    entries.push(synth(
        GET,
        pattern(
            prefix,
            &[Seg::Lit("remote"), Seg::Lit("control"), Seg::Lit("server")],
        ),
        SurfaceAffinity::None,
        &super::super::super::local::HANDLER,
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
        SINGLE,
        true,
    ));
    for (method, retry) in [(GET, RETRYABLE), (POST, SINGLE), (DELETE, SINGLE)] {
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
            retry,
            false,
        ));
    }
}
