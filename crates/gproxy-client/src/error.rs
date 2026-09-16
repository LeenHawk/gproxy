use crate::Backend;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid connection configuration: {0}")]
    InvalidConfig(&'static str),
    #[error("failed to parse proxy URL")]
    InvalidProxy(#[source] url::ParseError),
    #[error("backend {0:?} is not available in this build")]
    BackendUnavailable(Backend),
    #[cfg(all(feature = "reqwest", not(target_arch = "wasm32")))]
    #[error("reqwest client construction failed")]
    Reqwest(#[source] reqwest::Error),
    #[cfg(all(feature = "wreq", not(target_arch = "wasm32")))]
    #[error("wreq client construction failed")]
    Wreq(#[source] wreq::Error),
    #[cfg(not(target_arch = "wasm32"))]
    #[error("client construction task failed")]
    BuildTask(#[source] tokio::task::JoinError),
}
