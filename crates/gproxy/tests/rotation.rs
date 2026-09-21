//! Master-key rotation, against a real database.
//!
//! The assertion that matters is not "the rotation ran" but "the instance still
//! works afterwards, and only under the new key". So each test writes a real
//! credential through the sdk's own management family, rotates, and then
//! re-opens the database with each key in turn.

use gproxy::{
    Settings,
    config::{AdminOptions, TelemetryOptions},
    instance,
};
use gproxy_app::{
    AppConfig,
    config::{ConsoleConfig, MasterKey, MasterKeyConfig, StoreBackendConfig},
};
use gproxy_sdk::dto::{CredentialWrite, ProviderWrite};
use serde_json::json;

/// The same instance shape as `tests/commands.rs`, plus a master-key
/// configuration.
fn settings(directory: &std::path::Path, master_key: MasterKeyConfig) -> Settings {
    Settings {
        config: AppConfig {
            port: 0,
            data_dir: Some(directory.to_string_lossy().into_owned()),
            store: StoreBackendConfig::Sqlite {
                path: "gproxy.db".into(),
            },
            master_key,
            console: ConsoleConfig {
                enabled: false,
                path: None,
            },
            ..AppConfig::default()
        },
        admin: AdminOptions::default(),
        telemetry: TelemetryOptions::default(),
        instance_id: None,
    }
}

fn key(byte: u8) -> MasterKey {
    MasterKey::Hex(format!("{byte:02x}").repeat(32))
}

/// `key`, with no rotation.
fn steady(key: MasterKey) -> MasterKeyConfig {
    MasterKeyConfig {
        key,
        next: MasterKey::None,
        rotate: false,
    }
}

/// Write one provider and one credential whose secret is sealed by whatever
/// codec the handle was assembled with, and answer the credential's id.
async fn seed(instance: &instance::Instance) -> String {
    let manage = instance.app.gproxy().manage();
    let provider = manage
        .providers()
        .create(ProviderWrite {
            name: "upstream".into(),
            channel: "custom".into(),
            ..Default::default()
        })
        .await
        .expect("create a provider");
    let credential = manage
        .credentials()
        .create(CredentialWrite {
            provider_id: provider.id,
            auth_kind: "api_key".into(),
            secret: json!({ "api_key": "the-upstream-token" }),
            ..Default::default()
        })
        .await
        .expect("create a credential");
    instance.app.reload_all().await.expect("reload");
    credential.id
}

/// The plaintext behind a credential, read back through the handle's codec.
/// Fails when the codec cannot open the row, which is the whole point.
async fn reveal(instance: &instance::Instance, id: &str) -> gproxy::Result<serde_json::Value> {
    Ok(instance
        .app
        .gproxy()
        .manage()
        .credentials()
        .reveal_secret(id)
        .await?)
}

#[tokio::test]
async fn a_rotation_re_seals_every_credential_and_only_the_new_key_opens_them() {
    let directory = tempfile::tempdir().unwrap();

    // Start under the old key and write a credential.
    let old = settings(directory.path(), steady(key(0x11)));
    let instance = instance::open(&old, instance::OpenOptions::management())
        .await
        .unwrap();
    let credential = seed(&instance).await;
    assert_eq!(
        reveal(&instance, &credential).await.unwrap()["api_key"],
        "the-upstream-token"
    );
    instance.app.gproxy().shutdown();
    drop(instance);

    // Rotate to the new key.
    let rotating = settings(
        directory.path(),
        MasterKeyConfig {
            key: key(0x11),
            next: key(0x22),
            rotate: true,
        },
    );
    let rotated = instance::open(&rotating, instance::OpenOptions::management())
        .await
        .expect("rotate");
    let report = rotated.secrets.rotated.expect("a rotation happened");
    assert_eq!(report.credentials, 1);
    assert_eq!(report.api_keys, 0);
    assert_eq!(report.tokenizer_tokens, 0);
    // This process is now on the new key, which is what lets it keep serving.
    assert_eq!(rotated.secrets.active_key, Some([0x22_u8; 32]));
    assert_eq!(
        reveal(&rotated, &credential).await.unwrap()["api_key"],
        "the-upstream-token"
    );
    rotated.app.gproxy().shutdown();
    drop(rotated);

    // The promoted key opens the database.
    let promoted = settings(directory.path(), steady(key(0x22)));
    let after = instance::open(&promoted, instance::OpenOptions::management())
        .await
        .expect("open under the promoted key");
    assert!(after.secrets.rotated.is_none());
    assert_eq!(
        reveal(&after, &credential).await.unwrap()["api_key"],
        "the-upstream-token"
    );
    after.app.gproxy().shutdown();
    drop(after);

    // And the old key no longer does. Core opens every credential while it
    // assembles a snapshot, so this fails at assembly rather than on first use —
    // which is the failure an operator who forgot to promote the key will see.
    let stale = settings(directory.path(), steady(key(0x11)));
    assert!(
        instance::open(&stale, instance::OpenOptions::management())
            .await
            .is_err(),
        "the old key still opened the database"
    );
}

#[tokio::test]
async fn adopting_encryption_is_a_rotation_from_plaintext() {
    let directory = tempfile::tempdir().unwrap();

    // An instance that was started without a key, as the startup warning
    // describes.
    let plaintext = settings(directory.path(), steady(MasterKey::None));
    let instance = instance::open(&plaintext, instance::OpenOptions::management())
        .await
        .unwrap();
    assert!(instance.secrets.is_plaintext());
    let credential = seed(&instance).await;
    instance.app.gproxy().shutdown();
    drop(instance);

    // Seal what is already there.
    let sealing = settings(
        directory.path(),
        MasterKeyConfig {
            key: MasterKey::None,
            next: key(0x33),
            rotate: true,
        },
    );
    let sealed = instance::open(&sealing, instance::OpenOptions::management())
        .await
        .expect("seal an unencrypted database");
    assert_eq!(sealed.secrets.rotated.unwrap().credentials, 1);
    assert!(!sealed.secrets.is_plaintext());
    assert_eq!(
        reveal(&sealed, &credential).await.unwrap()["api_key"],
        "the-upstream-token"
    );
    sealed.app.gproxy().shutdown();
    drop(sealed);

    // And plaintext mode cannot read it any more, which is the point.
    assert!(
        instance::open(&plaintext, instance::OpenOptions::management())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rotation_bumps_the_revision_so_the_other_instances_reload() {
    let directory = tempfile::tempdir().unwrap();
    let before = {
        let instance = instance::open(
            &settings(directory.path(), steady(key(0x44))),
            instance::OpenOptions::management(),
        )
        .await
        .unwrap();
        let revision = instance.revision;
        instance.app.gproxy().shutdown();
        revision
    };

    let rotated = instance::open(
        &settings(
            directory.path(),
            MasterKeyConfig {
                key: key(0x44),
                next: key(0x55),
                rotate: true,
            },
        ),
        instance::OpenOptions::management(),
    )
    .await
    .unwrap();
    // Even with nothing to re-seal: the bump is how a peer learns to reload.
    let report = rotated.secrets.rotated.unwrap();
    assert_eq!(report.total(), 0);
    assert!(report.revision > before, "{} !> {before}", report.revision);
    rotated.app.gproxy().shutdown();
}

#[tokio::test]
async fn arming_rotation_without_a_next_key_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let error = instance::open(
        &settings(
            directory.path(),
            MasterKeyConfig {
                key: key(0x66),
                next: MasterKey::None,
                rotate: true,
            },
        ),
        instance::OpenOptions::management(),
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("--master-key-next / GPROXY_MASTER_KEY_NEXT"),
        "{error}"
    );
}

#[tokio::test]
async fn rotating_to_the_key_already_in_force_is_refused_rather_than_wasted() {
    let directory = tempfile::tempdir().unwrap();
    let error = instance::open(
        &settings(
            directory.path(),
            MasterKeyConfig {
                key: key(0x77),
                next: key(0x77),
                rotate: true,
            },
        ),
        instance::OpenOptions::management(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("nothing to rotate"), "{error}");
}

#[tokio::test]
async fn a_next_key_that_was_not_armed_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let instance = instance::open(
        &settings(
            directory.path(),
            MasterKeyConfig {
                key: key(0x88),
                next: key(0x99),
                rotate: false,
            },
        ),
        instance::OpenOptions::management(),
    )
    .await
    .unwrap();
    assert!(instance.secrets.rotated.is_none());
    assert_eq!(instance.secrets.active_key, Some([0x88_u8; 32]));
    instance.app.gproxy().shutdown();
}
