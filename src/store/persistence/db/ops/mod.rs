//! Trait-method implementations for the `db` backend (SeaORM ↔ records).
//!
//! Config-entity `upsert`s insert WITH an explicit id when a `Some(id)` row is
//! missing (seeding an empty store from a pinned import bundle — matches the
//! file backend). Caveat: on Postgres an explicit-PK insert does NOT advance
//! the identity sequence, so a LATER auto-id insert (admin API, M10) could
//! collide. Import seeding is collision-free (a fully-pinned bundle into an
//! EMPTY store → every insert is explicit); post-seed sequence sync on Postgres
//! is an M10/admin-API concern. SQLite self-heals (next auto id is max+1).

pub mod authz;
pub mod identity;
pub mod logs;
pub mod metrics;
pub mod provider;
pub mod routing;
pub mod settings;
pub mod tokenize;
pub mod transform;
pub mod usage;

pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `true` when the error is a unique-constraint violation, across dialects:
/// sqlite "UNIQUE constraint failed", postgres "duplicate key value violates
/// unique constraint", mysql "Duplicate entry".
pub(crate) fn is_unique_violation(e: &sea_orm::DbErr) -> bool {
    let msg = e.to_string().to_ascii_lowercase();
    msg.contains("unique") || msg.contains("duplicate")
}

/// Map a SeaORM error from an insert/update: a unique-constraint violation
/// becomes a [`ConflictError`] (→ 409); anything else passes through as-is.
pub(crate) fn conflict_if_unique(e: sea_orm::DbErr, msg: impl FnOnce() -> String) -> anyhow::Error {
    if is_unique_violation(&e) {
        crate::store::persistence::ConflictError::new(msg()).into()
    } else {
        anyhow::Error::new(e)
    }
}
