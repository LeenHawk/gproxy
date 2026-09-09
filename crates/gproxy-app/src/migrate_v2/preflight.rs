use std::path::Path;

use super::{V2ImportReport, apply, cipher, plan, report::ImportIssue, source};

pub(super) async fn run(
    config: &crate::Config,
    path: &Path,
    source_key: Option<&str>,
    dry_run: bool,
) -> Result<V2ImportReport, crate::AppError> {
    let data = source::read_control(path).await?;
    let source_cipher = cipher::V2Cipher::new(source_key)?;
    let mut plan = plan::prepare(data, &source_cipher);
    let mut report = V2ImportReport::planned(&plan, dry_run);
    if !report.issues.is_empty() {
        return Ok(report);
    }
    let validation = async {
        let store = gproxy_store::Store::open(gproxy_store::BackendConfig::Sqlite {
            path: ":memory:".into(),
        })
        .await?;
        let cipher = crate::key_rotation::prepare(&store, config.secret_keys()).await?;
        apply::configuration(&store, &cipher, &plan.data, &mut plan.counts).await?;
        crate::control::SnapshotControl::new(
            store,
            crate::control::RuntimeOverrides::from_config(config),
        )
        .await?;
        Ok::<_, crate::AppError>(())
    }
    .await;
    if let Err(error) = validation {
        report.issues.push(ImportIssue {
            entity: "configuration",
            row: "preflight".into(),
            reason: error.to_string(),
        });
    }
    Ok(report)
}
