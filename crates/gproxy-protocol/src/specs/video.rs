use crate::spec::Seg::{Lit, Param, ParamAction};
use crate::spec::{Affinity, OperationSpec, SettleMode};

use super::{DELETE, FAM_GEM, FAM_OAI, GET, NEVER, POST, ing, video_character_op, video_op};

pub(super) const CREATE_VIDEO: OperationSpec = video_op(&[
    ing(POST, &[Lit("v1"), Lit("videos")], FAM_OAI, NEVER),
    ing(
        POST,
        &[
            Lit("v1beta"),
            Lit("models"),
            ParamAction("model", "predictLongRunning"),
        ],
        FAM_GEM,
        NEVER,
    ),
]);

pub(super) const RETRIEVE_VIDEO: OperationSpec = OperationSpec {
    ingress: &[
        ing(
            GET,
            &[Lit("v1"), Lit("videos"), Param("id")],
            FAM_OAI,
            NEVER,
        ),
        ing(
            GET,
            &[Lit("v1beta"), Lit("operations"), Param("id")],
            FAM_GEM,
            NEVER,
        ),
        ing(
            GET,
            &[
                Lit("v1beta"),
                Lit("models"),
                Param("model"),
                Lit("operations"),
                Param("id"),
            ],
            FAM_GEM,
            NEVER,
        ),
    ],
    settle: SettleMode::OnCompletedStatus,
    affinity: Affinity::Resource("video"),
};

pub(super) const LIST_VIDEOS: OperationSpec =
    video_op(&[ing(GET, &[Lit("v1"), Lit("videos")], FAM_OAI, NEVER)]);

pub(super) const DELETE_VIDEO: OperationSpec = video_op(&[ing(
    DELETE,
    &[Lit("v1"), Lit("videos"), Param("id")],
    FAM_OAI,
    NEVER,
)]);

pub(super) const VIDEO_CONTENT: OperationSpec = video_op(&[ing(
    GET,
    &[Lit("v1"), Lit("videos"), Param("id"), Lit("content")],
    FAM_OAI,
    NEVER,
)]);

pub(super) const REMIX_VIDEO: OperationSpec = video_op(&[ing(
    POST,
    &[Lit("v1"), Lit("videos"), Param("id"), Lit("remix")],
    FAM_OAI,
    NEVER,
)]);

pub(super) const CREATE_VIDEO_CHARACTER: OperationSpec = video_character_op(&[ing(
    POST,
    &[Lit("v1"), Lit("videos"), Lit("characters")],
    FAM_OAI,
    NEVER,
)]);

pub(super) const GET_VIDEO_CHARACTER: OperationSpec = video_character_op(&[ing(
    GET,
    &[Lit("v1"), Lit("videos"), Lit("characters"), Param("id")],
    FAM_OAI,
    NEVER,
)]);

pub(super) const EDIT_VIDEO: OperationSpec = video_op(&[ing(
    POST,
    &[Lit("v1"), Lit("videos"), Lit("edits")],
    FAM_OAI,
    NEVER,
)]);

pub(super) const EXTEND_VIDEO: OperationSpec = video_op(&[ing(
    POST,
    &[Lit("v1"), Lit("videos"), Lit("extensions")],
    FAM_OAI,
    NEVER,
)]);
