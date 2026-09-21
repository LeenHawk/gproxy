//! Where a desktop instance keeps the two secrets it cannot derive: the master
//! key that seals every upstream credential, and the gateway key the embedded
//! data plane demands.
//!
//! # The store of record is the system keychain
//!
//! There is no operator here to set `GPROXY_MASTER_KEY`, so the shell mints
//! the master key itself on first run and puts it in the platform's credential
//! store through [`keyring`] — Secret Service on Linux, the Keychain on macOS,
//! the Credential Manager on Windows. A machine that has none of those falls
//! back, and the fallback is different for the two secrets on purpose:
//!
//! | Secret | Keychain | No keychain |
//! |---|---|---|
//! | master key | 32 bytes, minted on first run | **no key at all**, and the warning `gproxy` already has |
//! | gateway key | the minted token | a `0600` file beside the database |
//!
//! The master key does **not** fall back to a file. A key sitting next to the
//! database it protects is not encryption, it is a longer path to the same
//! plaintext, and describing it as "encrypted at rest" would be the only
//! dishonest thing in this crate. When there is no keychain the instance runs
//! exactly as the server does without `GPROXY_MASTER_KEY`, and says so in the
//! same words. The gateway key does fall back to a file, because it is a
//! different kind of secret: it grants use of *this* instance, the data plane
//! cannot run without it, and the alternative is a desktop app that refuses to
//! start on a machine with no keyring daemon.
//!
//! # Why a placement record exists
//!
//! A keychain entry can disappear — a reinstalled OS, a cleared login keyring,
//! a different user. If the shell simply looked and found nothing it would
//! conclude "first run", mint a *new* master key, and every credential already
//! in the database would become unopenable while the instance cheerfully
//! reported success. So the instance writes down which store its master key
//! went into ([`Placement`], in `secrets.json`), and a keychain that has lost
//! the entry it is supposed to hold is a refusal to start rather than a silent
//! re-mint. [`StartError::MasterKeyLost`](crate::StartError::MasterKeyLost) is
//! that refusal, and it names the entry to restore.
//!
//! The record holds no key material and no digest of one — only the name of a
//! store. v3 kept a `master_key_fingerprint` next to the data it protected;
//! `gproxy`'s rotation module explains why that is not reproduced.
//!
//! # Testing without a keychain
//!
//! Every function here takes a [`SecretStore`], and the keychain is one
//! implementation of it. The tests use [`MemoryStore`] and
//! [`UnavailableStore`], so the fallback, the re-mint refusal and the
//! placement record are all exercised on a machine with no D-Bus session at
//! all. Nothing in the test suite touches a real credential store.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

use gproxy_app::config::{MasterKey, MasterKeyConfig};
use serde::{Deserialize, Serialize};

use crate::{StartError, StartResult};

/// The service name every entry is filed under. It is the bundle identifier
/// from `tauri.conf.json`: one application, one service, so an uninstall that
/// sweeps the store by service takes both entries and nothing else.
pub const KEYCHAIN_SERVICE: &str = "dev.gproxy.desktop";

/// The master key's account name within the service.
pub const MASTER_KEY_ACCOUNT: &str = "master-key";

/// The gateway key's account name within the service.
pub const GATEWAY_KEY_ACCOUNT: &str = "gateway-key";

/// The placement record, relative to the data directory.
const PLACEMENT_FILE: &str = "secrets.json";

/// The gateway key's fallback file, relative to the data directory.
const GATEWAY_KEY_FILE: &str = "gateway-key";

/// A place a secret can be kept, so that the keychain is an implementation
/// rather than an assumption.
pub trait SecretStore: Send + Sync {
    /// The secret filed under `account`, or `None` when there is no such
    /// entry. An `Err` means the store itself could not be reached, which is
    /// **not** the same answer and must not be flattened into `None`.
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, secret: &str) -> Result<(), String>;
}

/// The platform credential store.
#[derive(Debug, Default, Clone, Copy)]
pub struct Keychain;

impl Keychain {
    fn entry(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|error| error.to_string())
    }
}

impl SecretStore for Keychain {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match Self::entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        Self::entry(account)?
            .set_password(secret)
            .map_err(|error| error.to_string())
    }
}

/// A store that holds nothing and remembers nothing, for tests and for the
/// machines this crate has to keep working on.
#[derive(Debug, Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().expect("poisoned").get(account).cloned())
    }
    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        self.0
            .lock()
            .expect("poisoned")
            .insert(account.to_owned(), secret.to_owned());
        Ok(())
    }
}

/// A store that is present but broken: every call fails, which is what a
/// machine with no keyring daemon actually looks like.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableStore;

impl SecretStore for UnavailableStore {
    fn get(&self, _account: &str) -> Result<Option<String>, String> {
        Err("no credential store on this platform".into())
    }
    fn set(&self, _account: &str, _secret: &str) -> Result<(), String> {
        Err("no credential store on this platform".into())
    }
}

/// Which store a secret ended up in. Recorded, never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    Keychain,
    /// For the master key: there is none, and secrets are stored in the clear.
    /// For the gateway key: a `0600` file in the data directory.
    Local,
}

impl Placement {
    pub fn is_keychain(self) -> bool {
        matches!(self, Self::Keychain)
    }
}

/// What the instance decided, once, and reads back on every later start.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    master_key: Placement,
    gateway_key: Placement,
}

/// The master key in force and where it came from.
pub struct MasterKeyOutcome {
    /// Handed straight to [`gproxy::Settings`]'s `AppConfig`, so the rotation
    /// and the handle assembly in `gproxy::instance` are the ones that run.
    pub config: MasterKeyConfig,
    pub placement: Placement,
}

impl std::fmt::Debug for MasterKeyOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately thin: `MasterKeyConfig` holds the key, and a `Debug`
        // that prints it is a `Debug` that prints it into the first log line
        // or test failure that formats the value.
        f.debug_struct("MasterKeyOutcome")
            .field("placement", &self.placement)
            .finish_non_exhaustive()
    }
}

/// Resolve the master key for the instance rooted at `data_dir`.
///
/// `fresh` says whether this is a database that does not exist yet. It is the
/// caller's answer rather than this module's because the caller is the one
/// that knows where the database file is, and getting it wrong in the "this is
/// new" direction is what the placement record exists to prevent.
pub fn master_key(
    store: &dyn SecretStore,
    data_dir: &Path,
    fresh: bool,
) -> StartResult<MasterKeyOutcome> {
    let record = read_record(data_dir);
    let known = record.map(|record| record.master_key);

    // An instance with a database but no record: this crate is looking at data
    // it did not write the record for. Ask the keychain rather than assuming,
    // because assuming "fresh" here is precisely the destructive mistake.
    let known = match (known, fresh) {
        (Some(placement), _) => Some(placement),
        (None, true) => None,
        (None, false) => match store.get(MASTER_KEY_ACCOUNT) {
            Ok(Some(_)) => Some(Placement::Keychain),
            _ => Some(Placement::Local),
        },
    };

    let outcome = match known {
        Some(Placement::Keychain) => match store.get(MASTER_KEY_ACCOUNT) {
            Ok(Some(hex)) => MasterKeyOutcome {
                config: MasterKeyConfig {
                    key: MasterKey::Hex(hex),
                    ..MasterKeyConfig::default()
                },
                placement: Placement::Keychain,
            },
            // Present, and empty. The database is sealed under a key nobody
            // has: refuse rather than start something that cannot open a
            // single credential.
            Ok(None) | Err(_) => {
                return Err(StartError::MasterKeyLost {
                    data_dir: data_dir.display().to_string(),
                    service: KEYCHAIN_SERVICE,
                    account: MASTER_KEY_ACCOUNT,
                });
            }
        },
        Some(Placement::Local) => MasterKeyOutcome {
            config: MasterKeyConfig::default(),
            placement: Placement::Local,
        },
        // First run. Mint, and keep it only if the keychain takes it.
        None => match mint_key().and_then(|hex| {
            store
                .set(MASTER_KEY_ACCOUNT, &hex)
                .map(|()| hex)
                .map_err(|error| error.to_string())
        }) {
            Ok(hex) => MasterKeyOutcome {
                config: MasterKeyConfig {
                    key: MasterKey::Hex(hex),
                    ..MasterKeyConfig::default()
                },
                placement: Placement::Keychain,
            },
            Err(error) => {
                tracing::debug!(%error, "the system keychain would not take the master key");
                MasterKeyOutcome {
                    config: MasterKeyConfig::default(),
                    placement: Placement::Local,
                }
            }
        },
    };

    write_placement(data_dir, |record| record.master_key = outcome.placement)?;
    Ok(outcome)
}

/// The gateway key the embedded data plane demands, or `None` when this
/// instance has not minted one yet.
pub fn gateway_key(store: &dyn SecretStore, data_dir: &Path) -> Option<String> {
    if let Ok(Some(token)) = store.get(GATEWAY_KEY_ACCOUNT) {
        return Some(token);
    }
    std::fs::read_to_string(data_dir.join(GATEWAY_KEY_FILE))
        .ok()
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

/// Keep `token` for the next start, in the keychain if there is one.
pub fn store_gateway_key(
    store: &dyn SecretStore,
    data_dir: &Path,
    token: &str,
) -> StartResult<Placement> {
    let placement = match store.set(GATEWAY_KEY_ACCOUNT, token) {
        Ok(()) => Placement::Keychain,
        Err(error) => {
            tracing::debug!(%error, "the system keychain would not take the gateway key");
            write_private(&data_dir.join(GATEWAY_KEY_FILE), token)?;
            Placement::Local
        }
    };
    write_placement(data_dir, |record| record.gateway_key = placement)?;
    Ok(placement)
}

/// 32 random bytes as 64 hexadecimal characters — the encoding
/// `gproxy::config` already accepts, so the desktop and the server hand
/// `MasterKey::Hex` exactly the same text.
fn mint_key() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| "secure randomness is unavailable on this machine".to_owned())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn read_record(data_dir: &Path) -> Option<Record> {
    let text = std::fs::read_to_string(data_dir.join(PLACEMENT_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Read, amend, write. Two fields are decided at different moments in one
/// start, and the second must not erase the first.
fn write_placement(data_dir: &Path, amend: impl FnOnce(&mut Record)) -> StartResult<()> {
    let mut record = read_record(data_dir).unwrap_or(Record {
        master_key: Placement::Local,
        gateway_key: Placement::Local,
    });
    amend(&mut record);
    let text = serde_json::to_string_pretty(&record).unwrap_or_default();
    let path = data_dir.join(PLACEMENT_FILE);
    std::fs::write(&path, text)
        .map_err(|error| StartError::io(format!("writing {}", path.display()), error))
}

/// Write a secret readable only by this account.
///
/// The mode is set *before* the bytes land, not after: a file created 0644 and
/// chmodded afterwards is world-readable for the window in between, and that
/// window is exactly when an indexer or a backup agent is most likely to be
/// walking a directory that just changed.
fn write_private(path: &Path, contents: &str) -> StartResult<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| StartError::io(format!("writing {}", path.display()), error))?;
    file.write_all(contents.as_bytes())
        .map_err(|error| StartError::io(format!("writing {}", path.display()), error))
}

/// The data directory's own path, for the error that names it.
pub fn placement_path(data_dir: &Path) -> PathBuf {
    data_dir.join(PLACEMENT_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_first_run_with_a_keychain_mints_a_key_and_records_where_it_went() {
        let data = dir();
        let store = MemoryStore::default();
        let outcome = master_key(&store, data.path(), true).unwrap();
        assert_eq!(outcome.placement, Placement::Keychain);
        let key = outcome.config.resolve().unwrap().expect("a key");

        // And the next start reads back the same key rather than minting a
        // second one, which would leave every sealed row unopenable.
        let again = master_key(&store, data.path(), false).unwrap();
        assert_eq!(again.config.resolve().unwrap(), Some(key));
    }

    #[test]
    fn a_first_run_without_a_keychain_falls_back_to_no_key_at_all() {
        let data = dir();
        let outcome = master_key(&UnavailableStore, data.path(), true).unwrap();
        assert_eq!(outcome.placement, Placement::Local);
        // Not a key in a file next to the database: no key, exactly as the
        // server behaves with no `GPROXY_MASTER_KEY`.
        assert_eq!(outcome.config.resolve().unwrap(), None);
        assert!(placement_path(data.path()).is_file());
    }

    #[test]
    fn an_instance_that_chose_plaintext_stays_plaintext_when_a_keychain_appears() {
        let data = dir();
        master_key(&UnavailableStore, data.path(), true).unwrap();
        // The machine grew a keychain between starts. The database is still
        // full of unsealed secrets, so minting a key now would seal nothing
        // and break every row that is already there.
        let outcome = master_key(&MemoryStore::default(), data.path(), false).unwrap();
        assert_eq!(outcome.placement, Placement::Local);
        assert_eq!(outcome.config.resolve().unwrap(), None);
    }

    #[test]
    fn a_keychain_that_lost_the_entry_refuses_to_start() {
        let data = dir();
        master_key(&MemoryStore::default(), data.path(), true).unwrap();
        // A fresh store stands in for a cleared login keyring: the record says
        // `keychain`, the keychain says nothing.
        let error = master_key(&MemoryStore::default(), data.path(), false).unwrap_err();
        assert!(
            matches!(error, StartError::MasterKeyLost { .. }),
            "{error:?}"
        );
        let text = error.to_string();
        assert!(text.contains(KEYCHAIN_SERVICE), "{text}");
        assert!(text.contains(MASTER_KEY_ACCOUNT), "{text}");
    }

    #[test]
    fn a_broken_keychain_on_an_existing_instance_is_not_read_as_a_first_run() {
        let data = dir();
        // No record on disk at all, and a database that already exists.
        let outcome = master_key(&UnavailableStore, data.path(), false).unwrap();
        assert_eq!(outcome.placement, Placement::Local);
        assert_eq!(outcome.config.resolve().unwrap(), None);
    }

    #[test]
    fn the_gateway_key_survives_a_restart_through_either_store() {
        let data = dir();
        let keychain = MemoryStore::default();
        assert_eq!(
            store_gateway_key(&keychain, data.path(), "sk-abc").unwrap(),
            Placement::Keychain
        );
        assert_eq!(
            gateway_key(&keychain, data.path()).as_deref(),
            Some("sk-abc")
        );

        let data = dir();
        assert_eq!(
            store_gateway_key(&UnavailableStore, data.path(), "sk-xyz").unwrap(),
            Placement::Local
        );
        assert_eq!(
            gateway_key(&UnavailableStore, data.path()).as_deref(),
            Some("sk-xyz")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(data.path().join(GATEWAY_KEY_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn deciding_the_gateway_key_does_not_erase_the_master_keys_record() {
        let data = dir();
        let store = MemoryStore::default();
        master_key(&store, data.path(), true).unwrap();
        store_gateway_key(&UnavailableStore, data.path(), "sk-abc").unwrap();
        let record = read_record(data.path()).unwrap();
        assert_eq!(record.master_key, Placement::Keychain);
        assert_eq!(record.gateway_key, Placement::Local);
    }

    #[test]
    fn an_unknown_instance_with_no_gateway_key_has_none() {
        assert!(gateway_key(&MemoryStore::default(), dir().path()).is_none());
    }
}
