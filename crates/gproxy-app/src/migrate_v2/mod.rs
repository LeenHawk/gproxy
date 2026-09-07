mod apply;
mod cipher;
mod metrics;
mod model;
mod plan;
mod report;
mod source;
mod tables;
mod tombstone;
pub(crate) mod upgrade;
mod validate;

use std::path::PathBuf;

use gproxy_store::records::SettingInput;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

pub use report::V2ImportReport;

pub struct V2ImportOptions {
    pub path: PathBuf,
    pub source_master_key: Option<String>,
    pub apply: bool,
    pub merge: bool,
}

impl std::fmt::Debug for V2ImportOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("V2ImportOptions")
            .field("path", &self.path)
            .field("source_master_key", &"<redacted>")
            .field("apply", &self.apply)
            .field("merge", &self.merge)
            .finish()
    }
}

impl Drop for V2ImportOptions {
    fn drop(&mut self) {
        if let Some(key) = &mut self.source_master_key {
            key.zeroize();
        }
    }
}

pub async fn migrate_from_v2(
    config: &crate::Config,
    options: V2ImportOptions,
) -> Result<V2ImportReport, crate::AppError> {
    if !options.path.is_file() {
        return Err(crate::AppError::Migration(
            "the v2 database path is not a readable file".into(),
        ));
    }
    let marker = source_marker(&options.path)?;
    let data = source::read(&options.path).await?;
    let source_cipher = cipher::V2Cipher::new(options.source_master_key.as_deref())?;
    let mut plan = plan::prepare(data, &source_cipher);
    let mut report = V2ImportReport {
        dry_run: !options.apply,
        applied: false,
        already_imported: false,
        counts: plan.counts.clone(),
        existing: Vec::new(),
        issues: plan.issues.clone(),
        skipped: plan.data.skipped.clone(),
    };
    if !options.apply || !report.issues.is_empty() {
        return Ok(report);
    }

    std::fs::create_dir_all(config.data_dir()).map_err(|error| {
        crate::AppError::Migration(format!("could not create target data directory: {error}"))
    })?;
    let store = gproxy_store::Store::open(config.backend_config()).await?;
    let snapshot = store.control_snapshot().await?;
    if snapshot
        .settings
        .iter()
        .any(|setting| setting.key == marker)
    {
        report.already_imported = true;
        return Ok(report);
    }
    report.existing = apply::existing(
        store.entity_counts().await?,
        user_setting_count(&snapshot.settings),
    );
    if !options.merge && !report.existing.is_empty() {
        report.issues.push(report::ImportIssue {
            entity: "target",
            row: "store".into(),
            reason: "is not empty; rerun with --merge to combine stores".into(),
        });
        return Ok(report);
    }

    let target_cipher = crate::key_rotation::prepare(&store, config.secret_keys()).await?;
    apply::run(
        &store,
        &target_cipher,
        plan.data,
        &mut plan.counts,
        &options.path,
    )
    .await?;
    store
        .import_settings(&[SettingInput {
            key: marker,
            value: serde_json::Value::Bool(true),
        }])
        .await?;
    report.counts = plan.counts;
    report.applied = true;
    Ok(report)
}

fn source_marker(path: &std::path::Path) -> Result<String, crate::AppError> {
    let path = path
        .canonicalize()
        .map_err(|_| crate::AppError::Migration("could not resolve the v2 database path".into()))?;
    let digest = Sha256::digest(path.as_os_str().as_encoded_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("v2_import_{hex}"))
}

pub(super) fn user_setting_count(settings: &[gproxy_store::records::SettingRecord]) -> usize {
    settings
        .iter()
        .filter(|setting| {
            setting.key != "master_key_fingerprint" && !setting.key.starts_with("v2_import_")
        })
        .count()
}
