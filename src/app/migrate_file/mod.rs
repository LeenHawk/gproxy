//! MIGRATE-FILE (temporary 2.x bridge, remove in 2.3): one-shot migration from
//! the removed 2.0.x JSON-file persistence backend into the configured db.
//!
//! Migrates control-plane configuration only. Usage/rollups, request and audit
//! logs, credential statuses, metrics, tokenizer caches, and `.gproxy.lock` are
//! intentionally not consumed. Successful imports rename each consumed config
//! table to `<name>.json.filebak`, making later boots a natural no-op.
//!
//! Import is explicit-id upsert and therefore retry-safe. On import failure no
//! JSON file is renamed; a small marker lets the next boot resume even if some
//! rows were written before the error. Backup renames are rolled back on error.

mod map;
mod read;
mod rows;

use std::collections::HashSet;
use std::path::Path;

use crate::app::import::{Bundle, import_bundle};
use crate::channel::registry::ChannelRegistry;
use crate::crypto::SecretCipher;
use crate::store::persistence::{DbPersistence, PersistenceBackend};

const MARKER: &str = ".gproxy.file-migrating";

#[derive(Debug, Default, Clone, Copy)]
pub struct Report {
    pub orgs: usize,
    pub teams: usize,
    pub users: usize,
    pub user_keys: usize,
    pub route_permissions: usize,
    pub rate_limits: usize,
    pub quotas: usize,
    pub providers: usize,
    pub credentials: usize,
    pub provider_models: usize,
    pub price_rules: usize,
    pub routes: usize,
    pub route_members: usize,
    pub aliases: usize,
    pub routing_rules: usize,
    pub rule_sets: usize,
    pub rules: usize,
    pub provider_rule_sets: usize,
    pub instance_settings: usize,
    pub total: usize,
}

impl Report {
    fn of(b: &Bundle) -> Self {
        let counts = [
            b.orgs.len(),
            b.teams.len(),
            b.users.len(),
            b.user_keys.len(),
            b.route_permissions.len(),
            b.rate_limits.len(),
            b.quotas.len(),
            b.providers.len(),
            b.credentials.len(),
            b.provider_models.len(),
            b.price_rules.len(),
            b.routes.len(),
            b.route_members.len(),
            b.aliases.len(),
            b.routing_rules.len(),
            b.rule_sets.len(),
            b.rules.len(),
            b.provider_rule_sets.len(),
            b.instance_settings.len(),
        ];
        Self {
            orgs: counts[0],
            teams: counts[1],
            users: counts[2],
            user_keys: counts[3],
            route_permissions: counts[4],
            rate_limits: counts[5],
            quotas: counts[6],
            providers: counts[7],
            credentials: counts[8],
            provider_models: counts[9],
            price_rules: counts[10],
            routes: counts[11],
            route_members: counts[12],
            aliases: counts[13],
            routing_rules: counts[14],
            rule_sets: counts[15],
            rules: counts[16],
            provider_rule_sets: counts[17],
            instance_settings: counts[18],
            total: counts.into_iter().sum(),
        }
    }
}

/// Automatically adopt legacy file-backend config on boot.
pub async fn maybe_migrate_on_boot(
    data_dir: &Path,
    db_dsn: &str,
    cipher: &dyn SecretCipher,
    channels: &ChannelRegistry,
) -> anyhow::Result<Option<Report>> {
    if db_dsn.starts_with("sqlite:")
        && crate::app::migration::sqlite_path_from_dsn(db_dsn).is_none()
    {
        return Ok(None);
    }
    if !read::legacy_tables_present(data_dir) {
        return Ok(None);
    }

    let marker = data_dir.join(MARKER);
    let resuming = marker.is_file();
    let target = DbPersistence::connect(db_dsn).await?;
    let configured =
        !target.list_providers().await?.is_empty() || !target.list_routes().await?.is_empty();
    if configured && !resuming {
        tracing::warn!(
            "legacy file-backend data found, but the target database already has providers or routes; skipping automatic migration (use export/import manually)"
        );
        target.close().await?;
        return Ok(None);
    }
    if !target.list_users().await?.is_empty() {
        tracing::warn!(
            "the target contains a bootstrap admin; legacy rows use explicit ids, so legacy administrator credentials now take effect"
        );
    }

    let data = read::read_all(data_dir)?;
    let consumed = data.consumed.clone();
    let bundle = map::to_bundle(data, cipher)?;
    let report = Report::of(&bundle);
    std::fs::write(&marker, b"in progress")?;
    let imported = apply(&target, cipher, channels, &bundle).await;
    if let Err(error) = imported {
        let _ = target.close().await;
        return Err(error);
    }
    target.close().await?;
    read::backup_consumed(&consumed)?;
    if let Err(error) = std::fs::remove_file(&marker) {
        tracing::warn!("could not remove completed file-migration marker: {error}");
    }
    tracing::warn!(
        ?report,
        "legacy file-backend configuration migrated; backups end in .filebak"
    );
    Ok(Some(report))
}

async fn apply(
    target: &DbPersistence,
    cipher: &dyn SecretCipher,
    channels: &ChannelRegistry,
    bundle: &Bundle,
) -> anyhow::Result<()> {
    let providers_with_rules: HashSet<i64> =
        bundle.routing_rules.iter().map(|r| r.provider_id).collect();
    let provider_ids: Vec<i64> = bundle.providers.iter().filter_map(|p| p.id).collect();
    import_bundle(target, cipher, &serde_json::to_string(bundle)?).await?;
    for provider_id in provider_ids {
        if !providers_with_rules.contains(&provider_id)
            && let Err(error) =
                crate::api::routing::seed_default_routing(target, channels, provider_id, true).await
        {
            tracing::warn!(
                provider_id,
                "could not seed default routing after file migration: {error:?}"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
