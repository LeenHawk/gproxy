//! Explicit state transitions. Inputs are trusted core/manage requests; each
//! method retains database preconditions needed against concurrent changes.
pub mod agents;
pub mod counted;
pub mod credentials;
pub mod cycles;
pub mod oauth;
mod oauth_policy;
pub mod quota;
pub mod rewrite;
mod sql;
pub mod state;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CasOutcome {
    Applied,
    Conflict,
}

pub(crate) fn cas(rows: u64) -> CasOutcome {
    if rows == 0 {
        CasOutcome::Conflict
    } else {
        CasOutcome::Applied
    }
}
