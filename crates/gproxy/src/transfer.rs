//! `export` and `import`: one instance's configuration as a JSON document.
//!
//! Both delegate to the sdk's `manage().transfer()`, which owns what travels and
//! what does not. Nothing is decided here beyond where the bytes come from and
//! go to, and how the master key is spelled on the way in.
//!
//! # Identity does not travel
//!
//! What moves is the configuration a deployment *is*: connection profiles,
//! providers, credentials, the model catalog, routing, operation overrides,
//! rewrite rules, quotas, pricing, settings. Users, keys, organizations, teams,
//! permissions and subscriptions do not, because they belong to the application
//! layer and the sdk cannot see them. Usage and captures do not either: copying
//! them would fabricate history the destination never had. So an import lands on
//! an instance that still needs its own [`crate::bootstrap`].
//!
//! # Secrets
//!
//! `--include-secrets` writes the sealed blobs base64-encoded, never opened and
//! never plaintext — the document is then exactly as sensitive as the database
//! file. On the way back in, `--source-master-key` lets the destination open and
//! re-seal them under its own key; without it the blobs are stored verbatim,
//! which only works when both instances share a key. Neither, and the credential
//! is skipped and counted, because one unopenable row would fail every reload of
//! the destination.

use std::{
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

use gproxy_app::App;
use gproxy_sdk::dto::{
    ConfigurationExportDto, ExportRequest, ImportMode, ImportReportDto, ImportRequest,
};
use gproxy_seaorm::BatchConnectionTrait;

use crate::{Error, Result, cli::ImportModeArg};

/// `-`, for the file arguments that accept a stream instead of a path.
const STREAM: &str = "-";

/// Write this instance's configuration to `out`, or to standard output for `-`.
pub async fn export<C>(app: &Arc<App<C>>, out: &Path, include_secrets: bool) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let document = app
        .gproxy()
        .manage()
        .transfer()
        .export(ExportRequest { include_secrets })
        .await?;
    // Pretty, with a trailing newline: an export gets committed to a repository
    // and read in a diff at least as often as it gets replayed.
    let mut text = serde_json::to_string_pretty(&document)
        .map_err(|error| Error::other(format!("serializing the export: {error}")))?;
    text.push('\n');
    write(out, text.as_bytes())?;

    let counts = &document.data;
    tracing::info!(
        providers = counts.providers.len(),
        credentials = counts.credentials.len(),
        models = counts.models.len(),
        routes = counts.routes.len(),
        secrets_omitted = document.secrets_omitted,
        "configuration exported"
    );
    if !include_secrets {
        tracing::info!(
            "credential secrets were not included; the destination will need a login for each \
             credential. Pass --include-secrets to carry the sealed blobs."
        );
    }
    Ok(())
}

/// Replay a configuration document into this instance, as one transaction.
pub async fn import<C>(
    app: &Arc<App<C>>,
    input: &Path,
    mode: ImportModeArg,
    source_master_key: Option<&str>,
) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let bytes = read(input)?;
    let export: ConfigurationExportDto = serde_json::from_slice(&bytes).map_err(|error| {
        Error::other(format!(
            "{} is not a gproxy configuration export: {error}",
            display(input)
        ))
    })?;

    let mode = ImportMode::from(mode);
    let report = app
        .gproxy()
        .manage()
        .transfer()
        .import(ImportRequest {
            export,
            mode,
            // The sdk reads this as standard base64; an operator who has their
            // key as hex should not have to convert it by hand.
            source_master_key: source_master_key.map(standard_base64).transpose()?,
        })
        .await?;

    // The snapshot the process is serving is a revision behind the transaction
    // that just landed.
    app.reload_all().await?;
    announce(&report, mode);
    Ok(())
}

/// What the import did, and — with more weight — what it declined to do.
fn announce(report: &ImportReportDto, mode: ImportMode) {
    tracing::info!(
        mode = ?mode,
        created = report.created,
        updated = report.updated,
        skipped = report.skipped,
        credentials_resealed = report.credentials_resealed,
        credentials_skipped = report.credentials_skipped,
        "configuration imported"
    );
    for warning in &report.warnings {
        // A skipped credential is the warning that matters: the row is there and
        // cannot authenticate, which looks like a routing fault later.
        tracing::warn!(warning, "import");
    }
}

/// A master key as an operator has it — hex or any base64 alphabet — in the one
/// encoding the sdk's import reads.
fn standard_base64(key: &str) -> Result<String> {
    use base64::Engine;
    use gproxy_app::config::MasterKey;

    let trimmed = key.trim();
    let candidate = if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        MasterKey::Hex(trimmed.to_owned())
    } else {
        MasterKey::Base64(trimmed.to_owned())
    };
    let bytes = candidate
        .resolve()
        .map_err(|error| {
            Error::config(
                "--source-master-key / GPROXY_IMPORT_SOURCE_MASTER_KEY",
                error,
            )
        })?
        .ok_or_else(|| {
            Error::config(
                "--source-master-key / GPROXY_IMPORT_SOURCE_MASTER_KEY",
                "a source master key must not be empty",
            )
        })?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn read(path: &Path) -> Result<Vec<u8>> {
    if path == Path::new(STREAM) {
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|error| Error::io("reading standard input", error))?;
        return Ok(bytes);
    }
    std::fs::read(path).map_err(|error| Error::io(format!("reading {}", display(path)), error))
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    if path == Path::new(STREAM) {
        let mut out = std::io::stdout().lock();
        out.write_all(bytes)
            .and_then(|()| out.flush())
            .map_err(|error| Error::io("writing standard output", error))?;
        return Ok(());
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| Error::io(format!("creating {}", parent.display()), error))?;
    }
    std::fs::write(path, bytes)
        .map_err(|error| Error::io(format!("writing {}", display(path)), error))
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn a_hex_source_key_becomes_the_standard_base64_the_sdk_reads() {
        let raw = [0x7b_u8; 32];
        let hex: String = raw.iter().map(|byte| format!("{byte:02x}")).collect();
        let expected = base64::engine::general_purpose::STANDARD.encode(raw);
        assert_eq!(standard_base64(&hex).unwrap(), expected);
    }

    #[test]
    fn a_url_safe_base64_source_key_is_re_encoded() {
        let raw = [0xfb_u8; 32];
        let url_safe = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
        let expected = base64::engine::general_purpose::STANDARD.encode(raw);
        assert_eq!(standard_base64(&url_safe).unwrap(), expected);
        // And a key already in the target encoding survives the round trip.
        assert_eq!(standard_base64(&expected).unwrap(), expected);
    }

    #[test]
    fn a_source_key_of_the_wrong_length_is_refused_before_the_import_starts() {
        let error = standard_base64("abcd").unwrap_err();
        assert!(
            error.to_string().starts_with("--source-master-key /"),
            "{error}"
        );
    }

    #[test]
    fn an_empty_source_key_is_not_a_key() {
        assert!(standard_base64("   ").is_err());
    }
}
