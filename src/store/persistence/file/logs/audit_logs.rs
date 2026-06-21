//! File-backend admin audit-log ops over `audit_logs.json` (append-only).

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{AuditLog, AuditLogInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("audit_logs.json")
}

pub(crate) async fn append(root: &Path, input: AuditLogInput) -> anyhow::Result<AuditLog> {
    let file = path(root);
    let mut t = table::load::<AuditLog>(&file).await?;
    let now = now_secs();

    let id = t.next_id;
    t.next_id += 1;
    let row = AuditLog {
        id,
        at: now,
        actor_id: input.actor_id,
        actor_name: input.actor_name,
        action: input.action,
        target: input.target,
        status: input.status,
        source_ip: input.source_ip,
        created_at: now,
    };
    t.rows.push(row.clone());

    table::store(&file, &t).await?;
    Ok(row)
}

pub(crate) async fn list(root: &Path, limit: u64) -> anyhow::Result<Vec<AuditLog>> {
    let mut rows = table::load::<AuditLog>(&path(root)).await?.rows;
    rows.sort_by_key(|r| std::cmp::Reverse(r.id));
    rows.truncate(limit as usize);
    Ok(rows)
}
