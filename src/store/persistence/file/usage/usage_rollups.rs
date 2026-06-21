//! File-backend usage-rollup ops over `usage_rollups.json` (accumulate).

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{UsageRollup, UsageRollupInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("usage_rollups.json")
}

pub(crate) async fn add(root: &Path, input: UsageRollupInput) -> anyhow::Result<UsageRollup> {
    let file = path(root);
    let mut t = table::load::<UsageRollup>(&file).await?;
    let now = now_secs();

    // Locate the existing bucket matching ALL dimensions (incl. None).
    let existing = t.rows.iter_mut().find(|r| {
        r.granularity == input.granularity
            && r.bucket_start == input.bucket_start
            && r.provider_id == input.provider_id
            && r.org_id == input.org_id
            && r.team_id == input.team_id
            && r.user_id == input.user_id
            && r.route_name == input.route_name
            && r.model == input.model
    });

    let stored = match existing {
        Some(row) => {
            row.requests += input.requests;
            row.input_tokens += input.input_tokens;
            row.output_tokens += input.output_tokens;
            row.cost += input.cost;
            row.updated_at = now;
            row.clone()
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let rollup = UsageRollup {
                id,
                granularity: input.granularity,
                bucket_start: input.bucket_start,
                provider_id: input.provider_id,
                org_id: input.org_id,
                team_id: input.team_id,
                user_id: input.user_id,
                route_name: input.route_name,
                model: input.model,
                requests: input.requests,
                input_tokens: input.input_tokens,
                output_tokens: input.output_tokens,
                cost: input.cost,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(rollup.clone());
            rollup
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn list(
    root: &Path,
    granularity: &str,
    from: i64,
    to: i64,
    user_id: Option<i64>,
) -> anyhow::Result<Vec<UsageRollup>> {
    Ok(table::load::<UsageRollup>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|r| {
            r.granularity == granularity
                && r.bucket_start >= from
                && r.bucket_start <= to
                && (user_id.is_none() || r.user_id == user_id)
        })
        .collect())
}
