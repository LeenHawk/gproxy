//! Explicit selected-target file operations; no routing or implicit retry.

mod copy;
mod crud;
mod upload;

use crate::{HttpBody, WireResponse, codec::CodecLimits, transform::TransformError};

pub use copy::copy_to_multipart;
pub use crud::{
    claude_get, claude_list, delete_empty, gemini_get, gemini_list, openai_get, openai_list,
};
pub use upload::{
    GeminiUploadProgress, GeminiUploadSession, UploadError, UploadFailure, gemini_start_resumable,
    gemini_upload_chunk, upload_multipart_json,
};

#[derive(Debug, Clone, Copy)]
pub struct FileCrudLimits {
    pub codec: CodecLimits,
    pub max_pages: usize,
    pub max_files: usize,
    pub max_declared_bytes: u64,
}

#[derive(Debug)]
pub enum FileFailure {
    Transform(TransformError),
    Rejected(Box<WireResponse<HttpBody>>),
}

#[derive(Debug)]
pub struct FileOperationError {
    pub completed_calls: usize,
    pub completed_files: usize,
    pub failure: FileFailure,
}

impl From<TransformError> for FileOperationError {
    fn from(value: TransformError) -> Self {
        Self {
            completed_calls: 0,
            completed_files: 0,
            failure: FileFailure::Transform(value),
        }
    }
}

impl std::fmt::Display for FileOperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "file operation failed after {} decoded calls and {} validated files",
            self.completed_calls, self.completed_files
        )
    }
}

impl std::error::Error for FileOperationError {}
pub(crate) fn path(path: &str) -> Result<(), TransformError> {
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['?', '#', '\\', '\r', '\n'])
    {
        return Err(TransformError::shape(
            "files.path",
            "origin-relative path required, query separate",
        ));
    }
    Ok(())
}

fn component(value: &str) -> String {
    let mut out = String::new();
    use std::fmt::Write;
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            write!(&mut out, "%{byte:02X}").unwrap();
        }
    }
    out
}

fn name(value: &str, gemini: bool) -> Result<String, TransformError> {
    let value = if gemini {
        value.strip_prefix("files/").ok_or_else(|| {
            TransformError::shape("files.name", "Gemini file resource must be files/{id}")
        })?
    } else {
        value
    };
    if value.is_empty() {
        return Err(TransformError::shape("files.id", "empty file id"));
    }
    Ok(if gemini {
        format!("files/{}", component(value))
    } else {
        component(value)
    })
}
