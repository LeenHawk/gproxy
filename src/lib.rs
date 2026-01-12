pub mod admin;
pub mod cli;
pub mod config;
pub mod context;
pub mod formats;
#[cfg(feature = "oauth")]
pub mod oauth;
pub mod providers;
pub mod public;
pub mod route;
pub mod storage;
pub mod usage;
#[cfg(feature = "front")]
pub mod front;

#[cfg(not(any(
    feature = "storage-file",
    feature = "storage-memory",
    feature = "storage-s3",
    feature = "storage-db",
)))]
compile_error!("At least one storage backend feature must be enabled.");

#[cfg(not(any(
    feature = "provider-openai",
    feature = "provider-claude",
    feature = "provider-claudecode",
    feature = "provider-codex",
    feature = "provider-aistudio",
    feature = "provider-vertex",
    feature = "provider-vertexexpress",
    feature = "provider-geminicli",
    feature = "provider-antigravity",
    feature = "provider-nvidia",
    feature = "provider-deepseek",
)))]
compile_error!("At least one provider feature must be enabled.");
