//! Helpers for classifying file-API operations.
//!
//! Several channels need to branch their behavior based on whether an
//! incoming request targets the Files API (upload / list / get /
//! content / delete) vs regular inference endpoints. These helpers
//! centralize that classification so every channel uses the same rule.

use gproxy_protocol::kinds::OperationFamily;

/// Returns true when the operation is one of the Files API endpoints.
pub fn is_file_operation(operation: OperationFamily) -> bool {
    matches!(
        operation,
        OperationFamily::FileUpload
            | OperationFamily::FileList
            | OperationFamily::FileContent
            | OperationFamily::FileGet
            | OperationFamily::FileDelete
    )
}

/// Returns true when the prepared request path belongs to a Files API endpoint.
pub fn is_file_operation_path(path: &str) -> bool {
    path.starts_with("/v1/files")
}
