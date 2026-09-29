//! Shared v3 data conversion, usable by native and Workers hosts.
use crate::{App, Operations};
use document::Document;
use gproxy_sdk::dto::{ImportMode, ImportRequest};
use gproxy_seaorm::BatchConnectionTrait;
pub use report::Report;
use sea_orm::EntityTrait;
use std::sync::Arc;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    App(#[from] crate::AppError),
    #[error(transparent)]
    Sdk(#[from] gproxy_sdk::SdkError),
    #[error(transparent)]
    Store(#[from] gproxy_store::StoreError),
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
}
impl Error {
    pub fn config(origin: &str, error: impl std::fmt::Display) -> Self {
        Self::Other(format!("{origin}: {error}"))
    }
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}
#[derive(Default)]
pub struct AdminOptions {
    pub user: String,
    pub password: Option<String>,
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub mod aliases;
pub mod channels;
pub mod config;
pub mod document;
pub mod endpoints;
pub mod fingerprint;
pub mod identity;
pub mod ids;
pub mod provider_config;
pub mod report;
pub mod routing;
pub mod rules;
pub mod secret;
pub mod settings;
pub mod source;
pub mod tokenizer;
pub mod upgrade;
pub mod usage;
pub async fn import_document<C>(
    app: &Arc<App<C>>,
    document: &Document,
    source_master_key: Option<&str>,
    admin: &AdminOptions,
    skip_unmappable: bool,
) -> Result<Report>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    refuse_a_populated_destination(app).await?;

    let bridge = secret::Bridge::new(match source_master_key {
        Some(key) => Some(secret::master_key(key)?),
        None => None,
    })?;
    check_the_key_matches_the_document(document, &bridge)?;

    let (mut report, dropped) =
        import_configuration(app, document, &bridge, skip_unmappable).await?;
    // The identity half: `gproxy-app`'s own families, one row at a time.
    report.absorb(identity::write(app, document, &bridge, &dropped).await?);
    admin_password(app, document, admin, &mut report).await?;
    app.reload_all().await?;
    Ok(report)
}
pub(crate) async fn import_configuration<C>(
    app: &Arc<App<C>>,
    document: &Document,
    bridge: &secret::Bridge,
    skip_unmappable: bool,
) -> Result<(Report, std::collections::BTreeSet<i64>)>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let translated = config::translate(document, bridge, now_ms(), skip_unmappable)?;
    let mut report = translated.report;

    // The configuration half: one sdk transaction, one revision bump, and the
    // sdk's own credential re-sealing.
    let sdk = app
        .gproxy()
        .manage()
        .transfer()
        .import(ImportRequest {
            export: translated.export,
            // Merge, never Replace: Replace deletes rows of an exported kind
            // the document does not mention, and on a destination this command
            // has already refused to share, there is nothing to delete.
            mode: ImportMode::Merge,
            source_master_key: Some(bridge.sdk_source_key()),
        })
        .await?;
    for warning in sdk.warnings {
        report.warn(warning);
    }
    if sdk.credentials_skipped > 0 {
        // Unreachable by construction — every credential is re-sealed under a
        // key this process just generated — so if it happens, say so loudly
        // rather than reporting a successful migration.
        return Err(Error::other(format!(
            "{} credentials were refused by the import after being re-sealed; this is a bug, \
             and the destination now holds a partial configuration",
            sdk.credentials_skipped
        )));
    }
    // v3's instance settings, as a patch over the destination's own.
    if let Some(patch) = settings::patch(&document.data.settings, &mut report) {
        app.gproxy().manage().settings().update(patch).await?;
    }
    tokenizer::write(app, document, bridge, &mut report).await?;
    app.reload_all().await?;

    Ok((report, translated.dropped_providers))
}
/// The "empty or nearly so" rule, made checkable.
///
/// The destination must hold no row this migration did not write. A row with a
/// `v3-` id is one of ours from an earlier run and is fine — that is what makes
/// a re-run the supported way to finish an interrupted import. Anything else is
/// somebody's configuration, and merging a whole v3 deployment into it would
/// produce a result neither side asked for and that no single command can undo.
///
/// Checked before the first write. A later write failure is recovered by
/// rerunning the same input, using the deterministic IDs.
async fn refuse_a_populated_destination<C>(app: &Arc<App<C>>) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    use gproxy_store::entity::{identity, limits, upstream};

    let store = app.gproxy().store();
    macro_rules! foreign {
        ($($what:literal => $repository:expr, $entity:path);* $(;)?) => {$({
            let rows = $repository.query(<$entity>::find()).await?;
            if let Some(row) = rows.iter().find(|row| !ids::is_migrated(&row.id)) {
                return Err(Error::other(format!(
                    "this instance already has configuration of its own: {} `{}`. \
                     `import --from-v3` replays a whole deployment and only runs against a \
                     database that is empty apart from an earlier run of itself. Point \
                     --data-dir at a fresh directory, run `gproxy migrate`, and import into \
                     that.",
                    $what, row.id
                )));
            }
        })*};
    }
    foreign! {
        "provider" => store.providers(), upstream::provider::Entity;
        "credential" => store.credentials(), upstream::credential::Entity;
        "route" => store.routes(), gproxy_store::entity::routing::route::Entity;
        "user" => store.users(), identity::user::Entity;
        "api key" => store.api_keys(), identity::api_key::Entity;
        "organization" => store.organizations(), identity::organization::Entity;
        "team" => store.teams(), identity::team::Entity;
        "permission" => store.permissions(), identity::permission::Entity;
        "rate limit" => store.rate_limits(), limits::rate_limit::Entity;
        "quota" => store.quotas(), limits::quota::Entity;
    }
    Ok(())
}

/// Catch the two ways the key and the document disagree before a single blob is
/// decrypted, because the message can be far more useful here than "did not
/// open".
fn check_the_key_matches_the_document(document: &Document, bridge: &secret::Bridge) -> Result<()> {
    let sealed = matches!(
        document.source_key,
        Some(document::SourceKey::Sealed { .. })
    );
    if sealed && !bridge.is_keyed() {
        return Err(Error::other(
            "this export says its secrets are sealed, and no key was given. Pass \
             --source-master-key with the value of GPROXY_MASTER_KEY on the v3 instance.",
        ));
    }
    if !sealed
        && bridge.is_keyed()
        && matches!(document.source_key, Some(document::SourceKey::Plaintext))
    {
        // Not fatal: the key is simply unused, and saying so beats leaving an
        // operator to wonder whether it was applied.
        tracing::warn!(
            "this export's secrets are not sealed, so --source-master-key was not needed"
        );
    }
    if document.secrets == document::Secrets::Omitted && !document.data.credentials.is_empty() {
        return Err(Error::other(
            "this export was taken without secrets, and it has credentials in it. A credential \
             without its secret cannot be created — core opens every one of them to assemble a \
             snapshot — so take the export again with {\"include_secrets\": true}.",
        ));
    }
    Ok(())
}

/// Give the migrated administrator a console password.
///
/// This exists because of a gap in v3's export rather than a design choice:
/// the document carries `users` but not `password_hash`, and v4 stores an
/// argon2 hash it cannot invent. Without this, a freshly migrated instance has
/// users, working API keys, and nobody who can sign in to the console —
/// `bootstrap admin` will not help, because its trigger is an *empty* users
/// table and the import just filled it.
///
/// So the administrator named by `--admin-user` (default `admin`), or the only
/// v3 administrator when the name does not match one, gets `--admin-password`.
/// Without the flag nothing is written and the report says what to do.
async fn admin_password<C>(
    app: &Arc<App<C>>,
    document: &Document,
    admin: &AdminOptions,
    report: &mut Report,
) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let Some(password) = admin.password.as_deref() else {
        if document
            .data
            .users
            .iter()
            .any(|user| user.is_admin && user.password_hash.is_none())
        {
            report.warn("an imported administrator has no password hash; use --admin-password when importing a JSON export".to_owned());
        }
        return Ok(());
    };
    let named = document
        .data
        .users
        .iter()
        .find(|row| row.name == admin.user && row.is_admin);
    let administrators: Vec<_> = document
        .data
        .users
        .iter()
        .filter(|row| row.is_admin)
        .collect();
    let chosen = match (named, administrators.as_slice()) {
        (Some(row), _) => row,
        (None, [only]) => only,
        (None, []) => {
            return Err(Error::other(
                "--admin-password was given, but the export has no administrator to give it to. \
                 Import without it and set a password from a console session.",
            ));
        }
        (None, many) => {
            return Err(Error::other(format!(
                "--admin-password was given, but `{}` is not one of this export's {} \
                 administrators ({}). Name one with --admin-user.",
                admin.user,
                many.len(),
                many.iter()
                    .map(|row| row.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    };

    let data = app.data();
    let operations = Operations::new(app.gproxy(), &data, app.config());
    operations
        .users()
        .set_password(&ids::id("users", chosen.id), password)
        .await?;
    report.warn(format!(
        "`{}` was given the supplied --admin-password",
        chosen.name
    ));
    Ok(())
}

fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}
