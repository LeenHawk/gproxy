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
//! Captures, logs, sessions, quota windows, credential health and audit history
//! stay in the backup. SQLite usage history is imported without re-settlement.
//! Beyond those, the v3
//! tables with no v4 form are reported row by row by [`report::Report`].

pub use gproxy_app::v3::aliases;
pub use gproxy_app::v3::channels;
pub use gproxy_app::v3::config;
pub mod detect;
pub use gproxy_app::v3::document;
pub use gproxy_app::v3::endpoints;
pub use gproxy_app::v3::fingerprint;
pub use gproxy_app::v3::identity;
pub use gproxy_app::v3::ids;
pub use gproxy_app::v3::provider_config;
pub use gproxy_app::v3::report;
pub use gproxy_app::v3::routing;
pub use gproxy_app::v3::rules;
pub use gproxy_app::v3::secret;
pub use gproxy_app::v3::settings;
pub mod source;
pub use gproxy_app::v3::tokenizer;
pub(crate) mod remote;
pub(crate) mod upgrade;

pub use report::Report;

/// Lowercase hex, for the source marker. One line rather than a dependency,
/// and the same encoding `gproxy-app` uses for a key digest.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

use std::{path::Path, sync::Arc};

use gproxy_app::App;
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
    let mut report = gproxy_app::v3::import_document(
        app,
        &document,
        source_master_key,
        &gproxy_app::v3::AdminOptions {
            user: admin.user.clone(),
            password: admin.password.clone(),
        },
        skip_unmappable,
    )
    .await?;
    if marker.is_some() {
        let connection = source::open(input).await?;
        let source = gproxy_app::v3::source::Source::new(&connection, "").await?;
        gproxy_app::v3::usage::import(app, &source, &mut report).await?;
        connection.close().await?;
    }
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
