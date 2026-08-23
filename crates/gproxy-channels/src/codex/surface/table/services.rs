use gproxy_channel_api::{SurfaceAffinity, SurfaceEntry};
use gproxy_protocol::Seg;

use super::{DELETE, GET, POST, SERVICE_PREFIXES, alias, pattern, synth};

pub(super) fn push(entries: &mut Vec<SurfaceEntry>) {
    v1(entries);
    service(entries);
}

fn v1(entries: &mut Vec<SurfaceEntry>) {
    entries.push(synth(
        GET,
        pattern(
            &[],
            &[
                Seg::Lit("v1"),
                Seg::Lit("user-auth-credential"),
                Seg::Lit("whoami"),
            ],
        ),
        SurfaceAffinity::None,
        &super::super::local::HANDLER,
        false,
    ));
    entries.push(alias(
        POST,
        pattern(
            &[],
            &[
                Seg::Lit("v1"),
                Seg::Lit("memories"),
                Seg::Lit("trace_summarize"),
            ],
        ),
        "/v1/memories/trace_summarize",
    ));
    entries.push(synth(
        GET,
        pattern(&[], &[Seg::Lit("v1"), Seg::Lit("files")]),
        SurfaceAffinity::None,
        &super::super::files::HANDLER,
        false,
    ));
    entries.push(synth(
        POST,
        pattern(&[], &[Seg::Lit("v1"), Seg::Lit("files")]),
        SurfaceAffinity::None,
        &super::super::files::HANDLER,
        true,
    ));
    for method in [GET, DELETE] {
        entries.push(synth(
            method,
            pattern(
                &[],
                &[Seg::Lit("v1"), Seg::Lit("files"), Seg::Param("file_id")],
            ),
            SurfaceAffinity::Binding {
                kind: super::super::helpers::FILE_KIND,
                param: "file_id",
            },
            &super::super::files::HANDLER,
            false,
        ));
    }
    entries.push(synth(
        GET,
        pattern(
            &[],
            &[
                Seg::Lit("v1"),
                Seg::Lit("files"),
                Seg::Param("file_id"),
                Seg::Lit("content"),
            ],
        ),
        SurfaceAffinity::Binding {
            kind: super::super::helpers::FILE_KIND,
            param: "file_id",
        },
        &super::super::files::HANDLER,
        true,
    ));
}

fn service(entries: &mut Vec<SurfaceEntry>) {
    for prefix in SERVICE_PREFIXES {
        entries.push(synth(
            GET,
            pattern(prefix, &[Seg::Lit("environments")]),
            SurfaceAffinity::None,
            &super::super::environments::HANDLER,
            true,
        ));
        entries.push(synth(
            GET,
            pattern(
                prefix,
                &[
                    Seg::Lit("environments"),
                    Seg::Lit("by-repo"),
                    Seg::Rest("repo"),
                ],
            ),
            SurfaceAffinity::None,
            &super::super::environments::HANDLER,
            true,
        ));
        entries.push(synth(
            GET,
            pattern(prefix, &[Seg::Lit("tasks"), Seg::Lit("list")]),
            SurfaceAffinity::None,
            &super::super::tasks::HANDLER,
            false,
        ));
        entries.push(synth(
            POST,
            pattern(prefix, &[Seg::Lit("tasks")]),
            SurfaceAffinity::None,
            &super::super::tasks::HANDLER,
            true,
        ));
        for method in [GET, POST] {
            entries.push(synth(
                method,
                pattern(prefix, &[Seg::Lit("tasks"), Seg::Param("task_id")]),
                SurfaceAffinity::Binding {
                    kind: super::super::helpers::TASK_KIND,
                    param: "task_id",
                },
                &super::super::tasks::HANDLER,
                true,
            ));
            entries.push(synth(
                method,
                pattern(
                    prefix,
                    &[
                        Seg::Lit("tasks"),
                        Seg::Param("task_id"),
                        Seg::Rest("task_rest"),
                    ],
                ),
                SurfaceAffinity::Binding {
                    kind: super::super::helpers::TASK_KIND,
                    param: "task_id",
                },
                &super::super::tasks::HANDLER,
                true,
            ));
        }
        entries.push(synth(
            POST,
            pattern(prefix, &[Seg::Lit("files")]),
            SurfaceAffinity::None,
            &super::super::files::HANDLER,
            true,
        ));
        entries.push(synth(
            POST,
            pattern(
                prefix,
                &[
                    Seg::Lit("files"),
                    Seg::Param("file_id"),
                    Seg::Lit("uploaded"),
                ],
            ),
            SurfaceAffinity::Binding {
                kind: super::super::helpers::FILE_KIND,
                param: "file_id",
            },
            &super::super::files::HANDLER,
            true,
        ));
        local(entries, prefix);
    }
}

fn local(entries: &mut Vec<SurfaceEntry>, prefix: &[&'static str]) {
    for (method, tail) in [
        (GET, &[Seg::Lit("usage")][..]),
        (
            POST,
            &[
                Seg::Lit("usage"),
                Seg::Lit("thread_usage"),
                Seg::Lit("query"),
            ][..],
        ),
        (GET, &[Seg::Lit("accounts"), Seg::Lit("check")][..]),
        (GET, &[Seg::Lit("profiles"), Seg::Lit("me")][..]),
        (GET, &[Seg::Lit("settings"), Seg::Lit("user")][..]),
        (GET, &[Seg::Lit("workspace-messages")][..]),
        (GET, &[Seg::Lit("config"), Seg::Lit("bundle")][..]),
        (GET, &[Seg::Lit("rate-limit-reset-credits")][..]),
        (
            POST,
            &[Seg::Lit("rate-limit-reset-credits"), Seg::Lit("consume")][..],
        ),
        (
            POST,
            &[
                Seg::Lit("accounts"),
                Seg::Lit("send_add_credits_nudge_email"),
            ][..],
        ),
        (
            POST,
            &[Seg::Lit("analytics-events"), Seg::Lit("events")][..],
        ),
    ] {
        entries.push(synth(
            method,
            pattern(prefix, tail),
            SurfaceAffinity::None,
            &super::super::local::HANDLER,
            false,
        ));
    }
}
