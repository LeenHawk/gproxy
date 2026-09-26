//! `gproxy import --from-v3`: a v3 deployment's configuration as a v4 one.
//!
//! # What an operator runs to produce the input
//!
//! v3 has no `export` subcommand — its only binary is the server. The document
//! this command reads comes from v3's admin API:
//!
//! ```text
//! POST /admin/api/export      {"include_secrets": true}
//! ```
//!
//! (`v3:crates/gproxy-admin/src/route.rs`, `Route::ConfigurationExport`.) It
//! answers a JSON object with `format_version: 1`, a `secrets` marker, a
//! `source_key` fingerprint and a `data` object of seventeen lists. See
//! [`document`] for the shape and the deployment page in `docs/` for the exact
//! commands, including the session login the endpoint requires.
//!
//! **Three v3 tables are missing from that document** — `permissions`,
//! `rate_limits` and `provider_models` — because v3's export handler simply
//! never collected them. All three have ordinary v3 list endpoints, so the
//! reader here accepts them as optional extra lists in the same object, in
//! v3's own DTO shape, and the documentation gives the `curl`/`jq` line that
//! splices them in.
//!
//! # The two halves, and why they are written differently
//!
//! Configuration goes through the sdk: the v3 document is translated into a
//! v4 [`ConfigurationExportDto`] and handed to `manage().transfer().import`,
//! which is one transaction and one revision bump. Identity has no sdk home —
//! it is `gproxy-app`'s — so it goes through [`gproxy_app::Operations`], the
//! same families the admin API calls, exactly as [`crate::bootstrap`] does.
//! Nothing here writes SQL.
//!
//! # Ids
//!
//! v3's are `i64` and v4's are `String`, so every row is minted as
//! `v3-{table}-{old_id}`; see [`ids`].
//!
//! # What is refused
//!
//! - a document that is not `format_version: 1`;
//! - a user key whose `digest_version` is not `1`, because no other version
//!   has ever existed and a row under an unknown digest can never match;
//! - a sealed document whose master key is missing or wrong, as a whole rather
//!   than a row at a time (see [`secret`]);
//! - a destination that already holds rows this migration did not write.
//!
//! The last one is the "empty or nearly so" rule made explicit. The check runs
//! before anything is written and names the table it found. Configuration and
//! identity are separate write phases; a failure after writing begins is
//! recovered by rerunning the same input. IDs are deterministic, so writes
//! target the rows the previous run made.
//!
//! # What is not migrated
//!
//! Usage records, captures, logs, sessions, login sessions, quota windows,
//! credential health and the audit trail: history, not configuration. Copying
//! them would fabricate a past this instance never had. Beyond those, the v3
//! tables with no v4 form are reported row by row by [`report::Report`].

pub mod aliases;
pub mod channels;
pub mod config;
pub mod detect;
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
pub(crate) mod upgrade;

pub use report::Report;

/// Lowercase hex, for the source marker. One line rather than a dependency,
/// and the same encoding `gproxy-app` uses for a key digest.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

use std::{path::Path, sync::Arc};

use gproxy_app::{App, Operations};
use gproxy_sdk::dto::{ImportMode, ImportRequest};
use gproxy_seaorm::BatchConnectionTrait;
use sea_orm::EntityTrait;

use crate::{Error, Result, config::AdminOptions};
use document::Document;

/// Read a v3 deployment and replay it into this instance.
///
/// `input` is either a **v3 SQLite database**, which is the route an operator
/// with a `gproxy.db` and a stopped service actually has, or a document from
/// v3's `POST /admin/api/export`. Which one it is is decided by looking at the
/// file, not by a flag: a SQLite file starts with a known 16-byte string, and
/// nothing else does.
///
/// The order is: read, refuse, translate, write the configuration through the
/// sdk, write the identity through the app, set one password, mark the source,
/// reload. The destination and credential keys are checked before writing;
/// an interruption during the write phases is recovered by rerunning the import.
pub async fn import<C>(
    app: &Arc<App<C>>,
    input: &Path,
    source_master_key: Option<&str>,
    admin: &AdminOptions,
    skip_unmappable: bool,
) -> Result<Report>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let (document, marker) = match is_sqlite(input)? {
        // The source file is never written to, so the "already imported" note
        // goes into the *destination*, keyed by the source's path. v3 did the
        // same thing one version earlier.
        true => (
            source::read(input).await?,
            Some(source::source_marker(input)?),
        ),
        false => (read_document(input)?, None),
    };
    if let Some(marker) = &marker
        && already_imported(app, marker).await?
    {
        tracing::info!(
            source = %input.display(),
            "this instance has already imported that v3 database; nothing to do"
        );
        let mut report = Report::default();
        report.warn(format!(
            "{} was already imported into this instance; nothing was changed. The completed import is recorded in the destination audit trail as `{marker}`.",
            input.display()
        ));
        return Ok(report);
    }
    refuse_a_populated_destination(app).await?;

    let bridge = secret::Bridge::new(match source_master_key {
        Some(key) => Some(secret::master_key(key)?),
        None => None,
    })?;
    check_the_key_matches_the_document(&document, &bridge)?;

    let translated = config::translate(&document, &bridge, now_ms(), skip_unmappable)?;
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
    app.reload_all().await?;

    // The identity half: `gproxy-app`'s own families, one row at a time.
    report.absorb(identity::write(app, &document, &bridge, &translated.dropped_providers).await?);
    admin_password(app, &document, admin, &mut report).await?;
    if let Some(marker) = marker {
        mark_imported(app, &marker).await?;
    }
    app.reload_all().await?;

    Ok(report)
}

/// Whether the file is a SQLite database rather than a JSON document. The
/// header string is SQLite's own and is the first 16 bytes of every file it
/// writes; a JSON document cannot begin with it.
fn is_sqlite(path: &Path) -> Result<bool> {
    use std::io::Read;
    const HEADER: &[u8; 16] = b"SQLite format 3\0";
    let mut file = std::fs::File::open(path)
        .map_err(|error| Error::io(format!("reading {}", path.display()), error))?;
    let mut head = [0_u8; 16];
    match file.read_exact(&mut head) {
        Ok(()) => Ok(&head == HEADER),
        // Too short to be a database; let the document reader say why.
        Err(_) => Ok(false),
    }
}

/// The action the source marker is recorded under.
const IMPORT_ACTION: &str = "migration.v3.import";
const IMPORT_ENTITY: &str = "v3_source";

/// Whether this instance has already imported that source.
///
/// v3 recorded its own v2 import as a `v2_import_{sha256(path)}` row in its
/// key/value `settings` table (`v3:crates/gproxy-app/src/migrate_v2/mod.rs`).
/// v4's `settings` is one wide row with fixed columns and has nowhere to put
/// such a key, so the same fact goes where v4 keeps "this happened": the audit
/// trail, which is append-only, indexed by `entity_id`, and survives the
/// deletion of everything the import created. The marker string itself is
/// v3's scheme with the version moved on.
async fn already_imported<C>(app: &Arc<App<C>>, marker: &str) -> Result<bool>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    use gproxy_store::entity::identity::audit_event;
    use sea_orm::{ColumnTrait, QueryFilter};
    let found = app
        .gproxy()
        .store()
        .audit_events()
        .query(
            audit_event::Entity::find()
                .filter(audit_event::Column::Action.eq(IMPORT_ACTION))
                .filter(audit_event::Column::EntityId.eq(marker))
                .filter(audit_event::Column::Outcome.eq(audit_event::OUTCOME_OK)),
        )
        .await?;
    Ok(!found.is_empty())
}

/// Record that this source has been imported. Written last, so a run that
/// failed halfway leaves no marker and the re-run that completes it is not
/// mistaken for a no-op.
async fn mark_imported<C>(app: &Arc<App<C>>, marker: &str) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    // This is an import idempotency marker, required even when audit logging is disabled.
    gproxy_app::Audit::new(app.gproxy().store(), true)
        .record(gproxy_app::AuditEntry {
            // Nobody signed in: this is the command line acting as the
            // instance itself.
            actor_user_id: None,
            actor_api_key_id: None,
            source_ip: None,
            action: IMPORT_ACTION.to_owned(),
            entity_kind: Some(IMPORT_ENTITY.to_owned()),
            entity_id: Some(marker.to_owned()),
            outcome: gproxy_store::entity::identity::audit_event::OUTCOME_OK.to_owned(),
            detail: serde_json::json!({"marker": marker}),
        })
        .await?;
    Ok(())
}

fn read_document(input: &Path) -> Result<Document> {
    let bytes = match input == Path::new("-") {
        true => {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|error| Error::io("reading standard input", error))?;
            bytes
        }
        false => std::fs::read(input)
            .map_err(|error| Error::io(format!("reading {}", input.display()), error))?,
    };
    // A v4 export spells the same field `formatVersion`, so it fails to parse
    // as a v3 one for a reason that says nothing useful. Recognise it first:
    // handing `export`'s own output to `--from-v3` is the likeliest mistake
    // anyone makes here, and the answer is one flag away.
    if let Ok(serde_json::Value::Object(fields)) = serde_json::from_slice(&bytes)
        && fields.contains_key("formatVersion")
    {
        return Err(Error::other(format!(
            "{} is a **v4** configuration export, not a v3 one. Replay it with \
             `gproxy import --in` instead.",
            input.display()
        )));
    }
    let document: Document = serde_json::from_slice(&bytes).map_err(|error| {
        Error::other(format!(
            "{} is not a v3 configuration export: {error}. Take one with \
             `POST /admin/api/export` on the v3 instance.",
            input.display()
        ))
    })?;
    if document.format_version != document::FORMAT_VERSION {
        return Err(Error::other(format!(
            "this document says format_version {}, and v3 only ever wrote {}. A v4 export goes \
             through `gproxy import --in` instead.",
            document.format_version,
            document::FORMAT_VERSION
        )));
    }
    Ok(document)
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
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}
