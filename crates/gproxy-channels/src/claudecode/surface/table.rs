use gproxy_channel_api::{
    ForwardRetry, ForwardSpec, SurfaceAction, SurfaceAffinity, SurfaceEntry, SurfaceTable,
    Synthesizer,
};
use gproxy_protocol::Seg::{Lit, Param};
use gproxy_protocol::{PathPattern, Seg};
use http::Method;

use super::helpers::{FILE_KIND, SKILL_KIND};

const GET: &Method = &Method::GET;
const POST: &Method = &Method::POST;
const DELETE: &Method = &Method::DELETE;

const fn synth(
    method: &'static Method,
    pattern: &'static [Seg],
    affinity: SurfaceAffinity,
    handler: &'static dyn Synthesizer,
    upstream: bool,
) -> SurfaceEntry {
    SurfaceEntry {
        method,
        pattern: PathPattern(pattern),
        affinity,
        action: SurfaceAction::Synthesize { handler, upstream },
    }
}

const fn forward(
    method: &'static Method,
    pattern: &'static [Seg],
    kind: &'static str,
    param: &'static str,
    label: &'static str,
    upstream_template: &'static str,
    retry: ForwardRetry,
) -> SurfaceEntry {
    SurfaceEntry {
        method,
        pattern: PathPattern(pattern),
        affinity: SurfaceAffinity::Binding { kind, param },
        action: SurfaceAction::Forward(ForwardSpec {
            label,
            upstream_template,
            retry,
        }),
    }
}

static ENTRIES: [SurfaceEntry; 23] = [
    synth(
        GET,
        &[Lit("api"), Lit("hello")],
        SurfaceAffinity::None,
        &super::local::HANDLER,
        false,
    ),
    synth(
        GET,
        &[Lit("api"), Lit("claude_cli"), Lit("bootstrap")],
        SurfaceAffinity::None,
        &super::local::HANDLER,
        false,
    ),
    synth(
        GET,
        &[Lit("api"), Lit("claude_cli_profile")],
        SurfaceAffinity::None,
        &super::local::HANDLER,
        false,
    ),
    synth(
        GET,
        &[Lit("api"), Lit("claude_code_penguin_mode")],
        SurfaceAffinity::None,
        &super::local::HANDLER,
        false,
    ),
    synth(
        GET,
        &[Lit("api"), Lit("claude_code"), Lit("skills")],
        SurfaceAffinity::None,
        &super::local::HANDLER,
        false,
    ),
    synth(
        POST,
        &[Lit("api"), Lit("oauth"), Lit("file_upload")],
        SurfaceAffinity::None,
        &super::files::HANDLER,
        true,
    ),
    forward(
        GET,
        &[
            Lit("api"),
            Lit("oauth"),
            Lit("files"),
            Param("file_id"),
            Lit("content"),
        ],
        FILE_KIND,
        "file_id",
        "claude_oauth_file_content",
        "/v1/files/{file_id}/content",
        ForwardRetry::Retryable,
    ),
    synth(
        GET,
        &[
            Lit("api"),
            Lit("oauth"),
            Lit("organizations"),
            Param("organization"),
            Lit("skills"),
            Lit("list-skills"),
        ],
        SurfaceAffinity::None,
        &super::local::HANDLER,
        false,
    ),
    synth(
        POST,
        &[
            Lit("api"),
            Lit("oauth"),
            Lit("organizations"),
            Param("organization"),
            Lit("skills"),
            Lit("search"),
        ],
        SurfaceAffinity::None,
        &super::local::HANDLER,
        false,
    ),
    synth(
        GET,
        &[
            Lit("api"),
            Lit("oauth"),
            Lit("organizations"),
            Param("organization"),
            Lit("skills"),
            Param("skill_id"),
            Lit("download"),
        ],
        SurfaceAffinity::None,
        &super::local::HANDLER,
        false,
    ),
    synth(
        GET,
        &[Lit("v1"), Lit("files")],
        SurfaceAffinity::None,
        &super::files::HANDLER,
        false,
    ),
    synth(
        POST,
        &[Lit("v1"), Lit("files")],
        SurfaceAffinity::None,
        &super::files::HANDLER,
        true,
    ),
    forward(
        GET,
        &[Lit("v1"), Lit("files"), Param("file_id")],
        FILE_KIND,
        "file_id",
        "claude_file_retrieve",
        "/v1/files/{file_id}",
        ForwardRetry::Retryable,
    ),
    synth(
        DELETE,
        &[Lit("v1"), Lit("files"), Param("file_id")],
        SurfaceAffinity::Binding {
            kind: FILE_KIND,
            param: "file_id",
        },
        &super::files::HANDLER,
        true,
    ),
    forward(
        GET,
        &[Lit("v1"), Lit("files"), Param("file_id"), Lit("content")],
        FILE_KIND,
        "file_id",
        "claude_file_content",
        "/v1/files/{file_id}/content",
        ForwardRetry::Retryable,
    ),
    synth(
        GET,
        &[Lit("v1"), Lit("skills")],
        SurfaceAffinity::None,
        &super::skills::HANDLER,
        false,
    ),
    synth(
        POST,
        &[Lit("v1"), Lit("skills")],
        SurfaceAffinity::None,
        &super::skills::HANDLER,
        true,
    ),
    forward(
        GET,
        &[Lit("v1"), Lit("skills"), Param("skill_id")],
        SKILL_KIND,
        "skill_id",
        "claude_skill_retrieve",
        "/v1/skills/{skill_id}",
        ForwardRetry::Retryable,
    ),
    synth(
        DELETE,
        &[Lit("v1"), Lit("skills"), Param("skill_id")],
        SurfaceAffinity::Binding {
            kind: SKILL_KIND,
            param: "skill_id",
        },
        &super::skills::HANDLER,
        true,
    ),
    forward(
        GET,
        &[Lit("v1"), Lit("skills"), Param("skill_id"), Lit("versions")],
        SKILL_KIND,
        "skill_id",
        "claude_skill_versions",
        "/v1/skills/{skill_id}/versions",
        ForwardRetry::Retryable,
    ),
    synth(
        POST,
        &[Lit("v1"), Lit("skills"), Param("skill_id"), Lit("versions")],
        SurfaceAffinity::Binding {
            kind: SKILL_KIND,
            param: "skill_id",
        },
        &super::skills::HANDLER,
        true,
    ),
    forward(
        GET,
        &[
            Lit("v1"),
            Lit("skills"),
            Param("skill_id"),
            Lit("versions"),
            Param("version_id"),
        ],
        SKILL_KIND,
        "skill_id",
        "claude_skill_version_retrieve",
        "/v1/skills/{skill_id}/versions/{version_id}",
        ForwardRetry::Retryable,
    ),
    forward(
        DELETE,
        &[
            Lit("v1"),
            Lit("skills"),
            Param("skill_id"),
            Lit("versions"),
            Param("version_id"),
        ],
        SKILL_KIND,
        "skill_id",
        "claude_skill_version_delete",
        "/v1/skills/{skill_id}/versions/{version_id}",
        ForwardRetry::SingleAttempt,
    ),
];

pub(super) fn table() -> SurfaceTable {
    SurfaceTable(&ENTRIES)
}
