use crate::spec::OperationSpec;
use crate::spec::Seg::{Lit, Param, ParamAction};

use super::{DELETE, FAM_GEM, FAM_OAI, GET, NEVER, POST, file_op, ing};

pub(super) const CREATE_FILE: OperationSpec = file_op(&[
    ing(POST, &[Lit("v1"), Lit("files")], FAM_OAI, NEVER),
    ing(
        POST,
        &[Lit("upload"), Lit("v1beta"), Lit("files")],
        FAM_GEM,
        NEVER,
    ),
]);

pub(super) const LIST_FILES: OperationSpec = file_op(&[
    ing(GET, &[Lit("v1"), Lit("files")], FAM_OAI, NEVER),
    ing(GET, &[Lit("v1beta"), Lit("files")], FAM_GEM, NEVER),
]);

pub(super) const RETRIEVE_FILE: OperationSpec = file_op(&[
    ing(GET, &[Lit("v1"), Lit("files"), Param("id")], FAM_OAI, NEVER),
    ing(
        GET,
        &[Lit("v1beta"), Lit("files"), Param("id")],
        FAM_GEM,
        NEVER,
    ),
]);

pub(super) const FILE_CONTENT: OperationSpec = file_op(&[
    ing(
        GET,
        &[Lit("v1"), Lit("files"), Param("id"), Lit("content")],
        FAM_OAI,
        NEVER,
    ),
    ing(
        GET,
        &[Lit("v1beta"), Lit("files"), ParamAction("id", "download")],
        FAM_GEM,
        NEVER,
    ),
    ing(
        GET,
        &[
            Lit("download"),
            Lit("v1beta"),
            Lit("files"),
            ParamAction("id", "download"),
        ],
        FAM_GEM,
        NEVER,
    ),
]);

pub(super) const DELETE_FILE: OperationSpec = file_op(&[
    ing(
        DELETE,
        &[Lit("v1"), Lit("files"), Param("id")],
        FAM_OAI,
        NEVER,
    ),
    ing(
        DELETE,
        &[Lit("v1beta"), Lit("files"), Param("id")],
        FAM_GEM,
        NEVER,
    ),
]);
