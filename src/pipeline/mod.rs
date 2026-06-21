//! Request lifecycle orchestration (§5, §6.3). Replaces v1's `engine.execute`
//! god function with small, testable steps threaded by a [`RequestCtx`].

pub mod auth;
pub mod authz;
pub mod balance;
pub mod capture;
pub mod classify;
pub mod context;
pub mod error;
pub mod execute;
pub mod failover;
mod health_hooks;
pub mod ingress;
pub mod local_ops;
pub mod outcome;
pub mod preprocess;
pub mod route;
pub mod settle;
#[cfg(not(target_arch = "wasm32"))]
pub mod stream;
#[cfg(all(
    test,
    not(target_arch = "wasm32"),
    feature = "persist-file",
    feature = "cache-memory"
))]
mod tests;
pub mod transform;

pub use context::{Candidate, Classified, RequestCtx, RoutingMode};
pub use error::PipelineError;
pub use execute::execute;
pub use outcome::{ExecOutcome, ResponseBody};
