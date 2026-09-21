//! The IPC error envelope, and the one rule it shares with the HTTP one.
//!
//! An IPC command answers `Result<T, IpcError>`, and Tauri serializes the
//! `Err` side straight to the webview. So this type is the desktop's half of
//! what [`gproxy_host_axum::ErrorResponse`] is on the wire: it carries
//! [`AppError::code`] and [`AppError::status_code`] and decides neither.
//!
//! Two things are deliberately kept identical to the HTTP envelope, because
//! the console is one codebase that branches on them:
//!
//! - the **code** is the same string, so a client that special-cases
//!   `rate_limited` or `not_found` does it once;
//! - a **5xx message is not quoted back**. It can hold a path, a row or a
//!   connection string, and a webview is a place a screenshot comes from. The
//!   caller gets the code, the log gets the text.
//!
//! What is *not* kept is the `{ "error": { … } }` wrapper. There is no
//! transport frame to disambiguate here — Tauri already tells the caller this
//! is the rejection — and wrapping again would make the console unwrap twice
//! on one branch and once on the other.

use gproxy_app::AppError;
use gproxy_sdk::SdkError;
use serde::Serialize;

/// One failed operation, as the webview sees it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    /// The stable machine-readable code, from `AppError::code`.
    pub code: String,
    pub message: String,
    /// The status the same failure would have produced over HTTP.
    ///
    /// There is no HTTP here, and it is carried anyway: it is the one number
    /// that already expresses "the request was wrong" versus "the instance
    /// was", and a console that renders both transports wants one rule for
    /// that rather than a list of codes per surface.
    pub status: u16,
}

impl IpcError {
    /// A failure raised by the shell itself rather than by an operation —
    /// a keychain that will not open, a socket that will not bind.
    pub fn internal(message: impl std::fmt::Display) -> Self {
        AppError::internal(message.to_string()).into()
    }
}

impl From<AppError> for IpcError {
    fn from(error: AppError) -> Self {
        let status = error.status_code();
        let message = if (500..600).contains(&status) {
            tracing::error!(%error, code = error.code(), "ipc command failed");
            "the instance failed to process this request".to_owned()
        } else {
            error.to_string()
        };
        Self {
            code: error.code().to_owned(),
            message,
            status,
        }
    }
}

impl From<SdkError> for IpcError {
    fn from(error: SdkError) -> Self {
        // Through `AppError` rather than directly, so an engine failure is
        // coded the same way whichever surface raised it. `AppError`'s own
        // conversion is what decides that a `429` from an upstream stays a
        // `429`.
        AppError::from(error).into()
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for IpcError {}

/// The result every command in the table returns.
pub type IpcResult<T> = std::result::Result<T, IpcError>;

/// A failure while starting the desktop instance.
///
/// Separate from [`IpcError`] because nothing is listening yet when one of
/// these happens: it is reported to the person who launched the application,
/// not to a webview.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error(transparent)]
    Cli(#[from] gproxy::Error),
    #[error(transparent)]
    App(#[from] AppError),
    #[error("{context}: {error}")]
    Io {
        context: String,
        #[source]
        error: std::io::Error,
    },
    /// The instance recorded that its master key lives in the system keychain,
    /// and the keychain no longer has it. Starting anyway would mean a handle
    /// that cannot open a single stored credential.
    #[error(
        "this instance's master key is stored in the system keychain and the keychain no longer \
         has it. Every upstream credential in {data_dir} is sealed under that key and cannot be \
         opened without it. Restore the keychain entry (service `{service}`, account \
         `{account}`), or delete the instance data and start again."
    )]
    MasterKeyLost {
        data_dir: String,
        service: &'static str,
        account: &'static str,
    },
}

impl StartError {
    pub fn io(context: impl Into<String>, error: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            error,
        }
    }
}

pub type StartResult<T> = std::result::Result<T, StartError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_code_is_the_one_the_http_surface_uses() {
        let error = IpcError::from(AppError::forbidden("no provider"));
        assert_eq!(error.code, "forbidden");
        assert_eq!(error.status, 403);
        assert!(error.message.contains("no provider"));
    }

    #[test]
    fn a_failure_of_this_instance_is_not_quoted_back_into_the_webview() {
        let error = IpcError::from(AppError::internal("/home/someone/gproxy.db"));
        assert_eq!(error.status, 500);
        assert_eq!(error.code, "internal_error");
        assert!(!error.message.contains("/home/someone"));
    }

    #[test]
    fn an_engine_failure_keeps_the_status_the_engine_chose() {
        let error = IpcError::from(SdkError::UnknownModel("nope".into()));
        assert_eq!(error.code, "unknown_model");
        assert_eq!(error.status, 404);
    }

    #[test]
    fn the_envelope_is_flat() {
        let value = serde_json::to_value(IpcError::from(AppError::NotFound {
            entity: "user",
            id: "u1".into(),
        }))
        .unwrap();
        assert_eq!(value["code"], "not_found");
        assert_eq!(value["status"], 404);
        // Not `{ "error": { … } }`: Tauri already framed this as the rejection.
        assert!(value["error"].is_null());
    }
}
