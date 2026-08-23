use crate::spec::OperationSpec;
use crate::spec::Seg::{Lit, Param};

use super::{DELETE, FAM_OAI, GET, NEVER, POST, file_op, ing};

pub(super) const CREATE_FILE: OperationSpec =
    file_op(&[ing(POST, &[Lit("v1"), Lit("files")], FAM_OAI, NEVER)]);

pub(super) const LIST_FILES: OperationSpec =
    file_op(&[ing(GET, &[Lit("v1"), Lit("files")], FAM_OAI, NEVER)]);

pub(super) const RETRIEVE_FILE: OperationSpec = file_op(&[ing(
    GET,
    &[Lit("v1"), Lit("files"), Param("id")],
    FAM_OAI,
    NEVER,
)]);

pub(super) const FILE_CONTENT: OperationSpec = file_op(&[ing(
    GET,
    &[Lit("v1"), Lit("files"), Param("id"), Lit("content")],
    FAM_OAI,
    NEVER,
)]);

pub(super) const DELETE_FILE: OperationSpec = file_op(&[ing(
    DELETE,
    &[Lit("v1"), Lit("files"), Param("id")],
    FAM_OAI,
    NEVER,
)]);
