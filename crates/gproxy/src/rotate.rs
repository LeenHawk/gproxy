//! Master-key rotation: re-sealing every secret in the database under a new
//! key, as one transaction.
//!
//! # What this is for
//!
//! `credentials.secret`, `api_keys.secret` and `settings.tokenizer_auth_token`
//! hold blobs sealed by [`gproxy_core::SecretCodec`] — AES-256-GCM under the
//! configured master key, or a plaintext envelope when there is no key. Changing
//! the key is therefore not a configuration edit: every one of those blobs has
//! to be opened with the old key and sealed again with the new one, or the next
//! reload fails on the first credential it cannot open.
//!
//! So rotation is two configured keys and a switch. `key` is what the instance
//! decrypts with today, `next` is what it re-seals to, and `rotate` performs it.
//! Afterwards the operator promotes `next` to `key` and clears both.
//!
//! # The three properties that make it safe
//!
//! **One transaction.** Every `UPDATE` and the revision bump go through
//! [`Store::commit_revision`] together. A failure anywhere leaves the database
//! entirely on the old key, so a rotation can always simply be retried — there
//! is never a half-rotated database to work out the shape of first.
//!
//! **Everything is opened before anything is written.** A key that decrypts most
//! of the rows and not one of them must not produce a database where most rows
//! are on the new key. Every blob is opened first, and a single failure aborts
//! before the first write.
//!
//! **Nothing is skipped.** A blob that cannot be opened is an error, never a
//! row left behind. A skipped row would be unopenable by *both* keys the moment
//! the operator promotes `next`, and the loss would only surface later.
//!
//! # What it deliberately does not do
//!
//! There is no durable record of which key the database is on. v3 kept a
//! `master_key_fingerprint` setting — an unsalted SHA-256 of the master key,
//! stored in plaintext next to the data it protected — and used it to refuse a
//! start with the wrong key. That check is not reproduced: the same protection
//! comes for free from the codec, because a wrong key fails to open the first
//! credential and the reload refuses, and storing a hash of the key buys the
//! operator nothing the failure does not already tell them.
//!
//! One consequence follows from having no marker, and it is the operational
//! rule: **rotate with one instance running.** A peer that is still holding the
//! old key sees the revision bump, reloads, and cannot open anything. Stop the
//! others, rotate, promote the key, start them again.

use gproxy_app::config::MasterKeyConfig;
use gproxy_core::{AesGcmCodec, PlaintextCodec, SecretCodec};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::{
    Store,
    entity::{config::setting, identity::api_key, upstream::credential},
};
use sea_orm::{EntityTrait, Set};
use serde_json::Value;

use crate::{Error, Result};

/// The id `settings.tokenizer_auth_token` is sealed under.
///
/// A sealed blob is bound to an id as authenticated data, so re-sealing needs
/// the same id the sdk used. That constant is `pub(super)` in
/// `gproxy_sdk::manage::settings`, so the literal is repeated here — and it is
/// safe to repeat it because getting it wrong is loud rather than silent: the
/// open fails and the rotation refuses before writing anything.
const TOKENIZER_SECRET_ID: &str = "settings:tokenizer_auth_token";

/// The fact, phrased once.
///
/// Two hosts have to say this — the server when no `GPROXY_MASTER_KEY` was
/// set, the desktop shell when the machine has no usable keychain — and the
/// remedy differs between them while the fact does not. So the sentence that
/// states what is true of the database lives here, and each host appends the
/// sentence that says what to do about it. Two independently worded warnings
/// about one condition is how an operator ends up believing they are two
/// conditions.
pub const PLAINTEXT_SECRETS: &str =
    "upstream credential secrets are stored UNENCRYPTED: no master key is configured.";

/// What the master-key configuration resolved to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// The key the handle must be assembled with. After a rotation this is the
    /// **new** key, because that is what the database now holds.
    pub active_key: Option<[u8; 32]>,
    /// The rotation that just happened, if one did.
    pub rotated: Option<Rotated>,
}

// Manual, because the derived one would print the master key. A `Debug` that
// leaks key material is a `Debug` that leaks it into the first log line or test
// failure that formats the value that holds it.
impl std::fmt::Debug for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outcome")
            .field("sealed", &self.active_key.is_some())
            .field("rotated", &self.rotated)
            .finish()
    }
}

impl Outcome {
    /// Whether secrets are stored unencrypted. The caller warns about it once at
    /// startup, naming the variable that would fix it.
    pub fn is_plaintext(&self) -> bool {
        self.active_key.is_none()
    }
}

/// How much a rotation moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rotated {
    pub credentials: usize,
    pub api_keys: usize,
    pub tokenizer_tokens: usize,
    /// The revision the re-seal committed at.
    pub revision: i64,
}

impl Rotated {
    pub fn total(&self) -> usize {
        self.credentials + self.api_keys + self.tokenizer_tokens
    }
}

/// Resolve the master-key configuration, performing the rotation it asks for.
///
/// Called once at startup, before the handle exists. The returned
/// [`Outcome::active_key`] is what the handle must use.
pub async fn run<C>(store: &Store<C>, config: &MasterKeyConfig) -> Result<Outcome>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let current = config.resolve()?;
    let Some(next) = config.resolve_next()? else {
        // A next key configured without arming the switch is almost always an
        // operator who expected it to take effect. Saying so is cheap; silently
        // serving on the old key for another month is not.
        if config.rotate {
            return Err(Error::config(
                "--master-key-next / GPROXY_MASTER_KEY_NEXT",
                "rotation was requested but no next key was configured",
            ));
        }
        if config.next != gproxy_app::config::MasterKey::None {
            tracing::warn!(
                "a next master key is configured but GPROXY_MASTER_KEY_ROTATE is off; nothing was \
                 rotated"
            );
        }
        return Ok(Outcome {
            active_key: current,
            rotated: None,
        });
    };

    if Some(next) == current {
        return Err(Error::config(
            "--master-key-next / GPROXY_MASTER_KEY_NEXT",
            "the next key is the key already in force; there is nothing to rotate",
        ));
    }

    let rotated = reseal(store, codec(current), codec(Some(next))).await?;
    tracing::warn!(
        credentials = rotated.credentials,
        api_keys = rotated.api_keys,
        tokenizer_tokens = rotated.tokenizer_tokens,
        revision = rotated.revision,
        "master key rotated; copy GPROXY_MASTER_KEY_NEXT to GPROXY_MASTER_KEY, then clear \
         GPROXY_MASTER_KEY_NEXT and GPROXY_MASTER_KEY_ROTATE before the next start"
    );
    Ok(Outcome {
        // The database is on the new key now, so this process has to be too.
        active_key: Some(next),
        rotated: Some(rotated),
    })
}

fn codec(key: Option<[u8; 32]>) -> Box<dyn SecretCodec> {
    match key {
        Some(key) => Box::new(AesGcmCodec::new(key)),
        None => Box::new(PlaintextCodec),
    }
}

/// Open every sealed blob with `from`, seal it again with `to`, and commit the
/// lot as one revision.
async fn reseal<C>(
    store: &Store<C>,
    from: Box<dyn SecretCodec>,
    to: Box<dyn SecretCodec>,
) -> Result<Rotated>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let credentials = store
        .credentials()
        .query(credential::Entity::find())
        .await?;
    let api_keys = store.api_keys().query(api_key::Entity::find()).await?;
    let settings = store.settings().get().await?;
    let (from, to) = (from.as_ref(), to.as_ref());

    let mut statements = Vec::new();
    let mut counts = Rotated {
        credentials: 0,
        api_keys: 0,
        tokenizer_tokens: 0,
        revision: 0,
    };

    for row in &credentials {
        // An empty blob is a credential with no secret — a row mid-creation, or
        // one whose channel needs none. There is nothing sealed to move.
        if row.secret.is_empty() {
            continue;
        }
        let secret = open(from, &row.id, &row.secret, "credential")?;
        statements.push(BatchStatement::Execute(
            store
                .credentials()
                .update_statement(credential::ActiveModel {
                    id: Set(row.id.clone()),
                    secret: Set(seal(to, &row.id, &secret, "credential")?),
                    ..Default::default()
                })?
                .ok_or_else(|| Error::other("a credential re-seal set no column"))?,
        ));
        counts.credentials += 1;
    }

    for row in &api_keys {
        // Only a key minted with `retainSecret` has one. The digest the key
        // authenticates under is not sealed and is not touched here.
        let Some(sealed) = row.secret.as_ref().filter(|blob| !blob.is_empty()) else {
            continue;
        };
        let secret = open(from, &row.id, sealed, "api key")?;
        statements.push(BatchStatement::Execute(
            store
                .api_keys()
                .update_statement(api_key::ActiveModel {
                    id: Set(row.id.clone()),
                    secret: Set(Some(seal(to, &row.id, &secret, "api key")?)),
                    ..Default::default()
                })?
                .ok_or_else(|| Error::other("an api key re-seal set no column"))?,
        ));
        counts.api_keys += 1;
    }

    if let Some(sealed) = settings
        .as_ref()
        .and_then(|row| row.tokenizer_auth_token.as_ref())
        .filter(|blob| !blob.is_empty())
    {
        let token = open(from, TOKENIZER_SECRET_ID, sealed, "tokenizer auth token")?;
        statements.push(BatchStatement::Execute(store.settings().update_statement(
            setting::ActiveModel {
                tokenizer_auth_token: Set(Some(seal(
                    to,
                    TOKENIZER_SECRET_ID,
                    &token,
                    "tokenizer auth token",
                )?)),
                ..Default::default()
            },
        )?));
        counts.tokenizer_tokens += 1;
    }

    // Commit even with nothing to re-seal. The revision bump is what tells the
    // other instances to reload, and a rotation of an empty database is still a
    // rotation: the operator promotes the key afterwards either way.
    counts.revision = store.commit_revision(statements).await?.revision;
    Ok(counts)
}

/// Open one blob, naming the row when it fails.
///
/// The codec's own error is deliberately uninformative — a wrong key, a
/// tampered blob and a foreign envelope all read the same — so the message here
/// adds what the operator needs and no more: which kind of row, and which id.
fn open(codec: &dyn SecretCodec, id: &str, sealed: &[u8], kind: &str) -> Result<Value> {
    codec.open(id, sealed).map_err(|_| {
        Error::other(format!(
            "{kind} `{id}` cannot be opened with the current master key; rotation was abandoned \
             and nothing was written"
        ))
    })
}

fn seal(codec: &dyn SecretCodec, id: &str, secret: &Value, kind: &str) -> Result<Vec<u8>> {
    codec
        .seal(id, secret)
        .map_err(|error| Error::other(format!("sealing {kind} `{id}`: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_app::config::MasterKey;

    fn key(byte: u8) -> MasterKey {
        MasterKey::Hex(format!("{byte:02x}").repeat(32))
    }

    #[test]
    fn an_unarmed_next_key_is_not_a_rotation() {
        let config = MasterKeyConfig {
            key: key(0x11),
            next: key(0x22),
            rotate: false,
        };
        assert_eq!(config.resolve_next().unwrap(), None);
    }

    #[test]
    fn a_blob_sealed_under_one_key_does_not_open_under_another() {
        let secret = Value::String("upstream-token".into());
        let old = AesGcmCodec::new([0x11; 32]);
        let new = AesGcmCodec::new([0x22; 32]);
        let sealed = old.seal("cred-1", &secret).unwrap();
        assert!(new.open("cred-1", &sealed).is_err());
        // Which is exactly what rotation fixes.
        let resealed = new
            .seal("cred-1", &old.open("cred-1", &sealed).unwrap())
            .unwrap();
        assert_eq!(new.open("cred-1", &resealed).unwrap(), secret);
    }

    #[test]
    fn the_seal_is_bound_to_the_row_it_came_from() {
        let codec = AesGcmCodec::new([0x33; 32]);
        let sealed = codec.seal("cred-1", &Value::String("x".into())).unwrap();
        // Which is why the re-seal has to know each row's id, and why the
        // tokenizer token's id has to be the one the sdk sealed it under.
        assert!(codec.open("cred-2", &sealed).is_err());
    }

    #[test]
    fn adopting_encryption_is_a_rotation_from_plaintext() {
        let secret = Value::String("upstream-token".into());
        let plaintext = PlaintextCodec;
        let sealed = plaintext.seal("cred-1", &secret).unwrap();
        let keyed = AesGcmCodec::new([0x44; 32]);
        let resealed = keyed
            .seal("cred-1", &plaintext.open("cred-1", &sealed).unwrap())
            .unwrap();
        assert_eq!(keyed.open("cred-1", &resealed).unwrap(), secret);
        assert!(plaintext.open("cred-1", &resealed).is_err());
    }

    #[test]
    fn a_total_counts_every_family() {
        let rotated = Rotated {
            credentials: 3,
            api_keys: 2,
            tokenizer_tokens: 1,
            revision: 9,
        };
        assert_eq!(rotated.total(), 6);
    }
}
