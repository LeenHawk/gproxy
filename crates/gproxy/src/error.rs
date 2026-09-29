//! One error type for the whole binary, and one place that decides what an
//! operator sees.
//!
//! The libraries below have their own taxonomies — `AppError` carries an HTTP
//! status, `SdkError` carries one too — but a command line has no status codes,
//! only a message and an exit code. So everything is flattened here, and the
//! only distinction kept is the one an operator can act on: a configuration
//! mistake is named with the flag and the environment variable that would fix
//! it, while everything else is reported as it came.

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Migration(#[from] gproxy_app::v3::Error),
    /// A value the operator supplied is wrong. `origin` names where it came
    /// from — `--port / GPROXY_PORT` — so the message points at the thing to
    /// change rather than at the code that rejected it.
    #[error("{origin}: {message}")]
    Config { origin: String, message: String },

    #[error("{context}: {error}")]
    Io {
        context: String,
        #[source]
        error: std::io::Error,
    },

    #[error(transparent)]
    App(#[from] gproxy_app::AppError),

    #[error(transparent)]
    Sdk(#[from] gproxy_sdk::SdkError),

    #[error(transparent)]
    Store(#[from] gproxy_store::StoreError),

    #[error("could not open the database: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("{0}")]
    Other(String),
}

impl Error {
    /// A rejected configuration value. `origin` is the flag and environment
    /// variable pair, in the form `--port / GPROXY_PORT`.
    pub fn config(origin: impl Into<String>, message: impl fmt::Display) -> Self {
        Self::Config {
            origin: origin.into(),
            message: message.to_string(),
        }
    }

    pub fn io(context: impl Into<String>, error: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            error,
        }
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}
