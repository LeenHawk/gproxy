//! Tests for [`DbPersistence`], grouped by persistence responsibility.

use super::DbPersistence;
use crate::store::persistence::records::*;
use crate::store::persistence::{AuditLogQuery, PageQuery, UsageQuery};
use serde_json::json;

async fn mem() -> DbPersistence {
    DbPersistence::connect("sqlite::memory:")
        .await
        .expect("connect")
}

mod audio_migration;
mod import;
mod migrations;
mod observability;
mod operations;
mod rerank_migration;
