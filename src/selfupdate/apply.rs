//! Update artifact validation and installation (§19.5-§19.7).

use super::*;

/// The latest data/schema version this binary operates at — the value the
/// running process has already migrated the store to on boot. Checked against
/// the target manifest's `min_compatible_data_version` (§19.7), which is the
/// oldest schema the target binary can migrate/read from. `0` in a
/// no-persistence build, so the check simply never fires there.
fn current_data_version() -> u32 {
    #[cfg(feature = "persist-db")]
    let v = crate::store::persistence::migrations::latest_version().max(0) as u32;
    #[cfg(not(feature = "persist-db"))]
    let v = 0u32;
    v
}

/// Download, verify (sha256 + ed25519 signature), atomically swap, and (per
/// `restart`) hand off to a new binary (§19.5 / §19.6). Returns the new
/// version/identity on success when no restart is requested.
///
/// `ReExec` does not return on success (it replaces the process image);
/// `Supervisor` exits the process with the sentinel code after staging.
pub async fn apply(ctx: &UpdateContext, restart: Restart) -> Result<String, UpdateError> {
    apply_with_options(
        ctx,
        ApplyOptions {
            restart,
            allow_insecure: false,
        },
    )
    .await
}

pub async fn apply_with_options(
    ctx: &UpdateContext,
    options: ApplyOptions,
) -> Result<String, UpdateError> {
    let manifest = download::fetch_manifest(ctx).await?;
    let triple = current_target_triple();
    let artifact = manifest
        .artifact_for(&triple)
        .ok_or_else(|| UpdateError::NoArtifact(triple.clone()))?
        .clone();
    let artifact_sha = artifact.sha256_value().map(str::to_string);

    // §19.7 data-compat floor (any channel): refuse a binary that no longer
    // supports the schema version this process has already migrated the store
    // to. Normal forward migrations are allowed: the new binary runs them on
    // restart. When the manifest is signed, this field is covered by that
    // signature.
    let required = manifest.min_compatible_data_version;
    let have = current_data_version();
    if required > have {
        return Err(UpdateError::Incompatible(format!(
            "manifest needs existing data version >= {required}, but this binary migrated \
             the store to {have}; update through an intermediate release first"
        )));
    }

    // The running build's commit identity drives both the staging
    // already-up-to-date gate and rollback guard, so resolve it once.
    let local_identity = match ctx.channel {
        Channel::Staging => Some(staging_current_identity()?),
        Channel::Releases => None,
    };

    // Gate: only proceed if there is actually something to install.
    let available = match &local_identity {
        Some(local) => version::staging_decision(local, &manifest.version).available,
        None => version::releases_decision(&manifest.version)?.available,
    };
    if !available {
        tracing::info!(channel = ctx.channel.as_str(), "already up to date");
        return Ok(manifest.version.clone());
    }

    let safety = safety_risks(&manifest, &artifact);
    if !safety.is_empty() {
        if !options.allow_insecure {
            return Err(UpdateError::ConfirmationRequired(safety_message(&safety)));
        }
        tracing::warn!(
            risks = ?safety,
            channel = ctx.channel.as_str(),
            "applying update with missing safety metadata after operator confirmation"
        );
    }

    // Staging rollback guard (§19.3): commit SHAs have identity but no ordering,
    // so a replayed older-but-validly-signed manifest could roll the binary
    // backward. Refuse an identity we've already superseded. `releases` is
    // ordered by semver vs the compiled-in version and needs no ledger.
    if let Some(local) = &local_identity
        && applied::is_rollback(&applied::load(&ctx.data_dir), local, &manifest.version)
    {
        return Err(UpdateError::Downgrade(format!(
            "staging build {} was already superseded by a newer build",
            applied::short(&manifest.version)
        )));
    }

    // 1. Download the release package to private staging storage.
    let staged_package = download::download_artifact(ctx, &artifact).await?;

    // 2. Integrity: sha256 of the downloaded package must equal the manifest's
    //    when the manifest provides one. Missing sha256 reached here only after
    //    explicit operator confirmation.
    if let Some(sha) = artifact_sha.as_deref() {
        verify::verify_sha256(&staged_package, sha)?;
    }

    // 3. Signature: the embedded ed25519 public key must verify the manifest
    //    signature when both are present. Missing signature/pubkey reached here
    //    only after explicit operator confirmation.
    if manifest.signature_value().is_some() && verify::has_embedded_pubkey() {
        verify::verify_manifest_signature(&manifest)?;
    }

    // 4/5. APK installations hand the verified package to the Java wrapper,
    // which invokes Android's system installer after this child exits. Other
    // installations extract and atomically replace the executable (plus the
    // Android portable archive's shared C++ runtime).
    if version::is_android_apk_installation() {
        let apk = android_apk::stage(&ctx.data_dir, &staged_package)?;
        tracing::info!(?apk, "verified Android package staged for system installer");
    } else {
        // When safety metadata is present, the archive's bytes were checked and
        // bound by the manifest signature, so extracted files inherit trust.
        let staged_bin = extract::extract_binary(&staged_package, &triple)?;
        swap::install(&staged_bin)?;
    }
    // Record the applied identity so a later replay of this (now-superseded)
    // build is caught by the rollback guard above. Staging only; best-effort.
    if let Some(local) = &local_identity {
        applied::record(&ctx.data_dir, local, &manifest.version);
    }
    tracing::info!(
        channel = ctx.channel.as_str(),
        version = %manifest.version,
        "update artifact staged"
    );

    // 6. Restart / hand off (§19.6).
    match options.restart {
        Restart::Supervisor => swap::exit_for_supervisor(),
        Restart::ReExec => restart_now(Restart::ReExec), // diverges on success
        Restart::None => Ok(manifest.version.clone()),
    }
}
