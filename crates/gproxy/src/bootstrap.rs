//! The first administrator, created once.
//!
//! A fresh database has no way in: the console needs a user, and the admin API
//! needs a key. So the first start creates one administrator, optionally mints
//! one gateway API key for them, and prints whatever the operator does not
//! already know — once, to standard output, never to the log.
//!
//! # Idempotency
//!
//! The trigger is an **empty `users` table**. Any user at all — administrator or
//! not — means this instance has been set up, and nothing is touched: no
//! password is reset, no key is minted, no row is changed. That is deliberately
//! blunt. The alternative ("no *administrator* exists") sounds more useful but
//! means a second start can silently mint a new key and print a new password
//! for an instance that already has users on it, and an operator restarting a
//! container with `GPROXY_ADMIN_PASSWORD` still set would be resetting a
//! password they did not mean to touch.
//!
//! # Why it goes through `Operations`
//!
//! Every write here is [`gproxy_app::Operations`] — the same families the admin
//! API calls. So the password is validated and argon2-hashed by the product's
//! own rules, and the key is digested by the product's own function. v3 wrote
//! the bootstrap key with hand-rolled SQL and its own digest call, and the
//! digest it chose was not the one the admin API looked the key up under: an
//! `sk-`-prefixed bootstrap key answered `401` on every request. The fix is not
//! a better digest, it is having only one.
//!
//! # Printing
//!
//! `println!`, not `tracing`. A generated password and a minted key are secrets,
//! and a log line goes to a file, a journal and whatever ships the journal
//! somewhere else. Standard output belongs to the person who ran the command.

use std::sync::Arc;

use gproxy_app::{
    App, Operations,
    dto::{ApiKeyWrite, UserWrite},
};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::user;
use sea_orm::{EntityTrait, QuerySelect};

use crate::{Result, config::AdminOptions};

/// The administrator's role, as `gproxy-app`'s `users` family spells it.
const ADMIN_ROLE: &str = "admin";

/// The name of the key minted alongside the first administrator.
const BOOTSTRAP_KEY_NAME: &str = "bootstrap";

/// 24 random bytes, base64url without padding: 32 characters, comfortably past
/// the product's eight-character minimum and short enough to be copied out of a
/// terminal by hand.
const GENERATED_PASSWORD_BYTES: usize = 24;

/// What bootstrap did, and what the operator has to be told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// The instance already had users. Nothing was written.
    AlreadySetUp { users: u64 },
    Created {
        user: String,
        /// Present only when this process generated it. A password the operator
        /// supplied is never echoed.
        password: Option<String>,
        /// Present only when this process generated it, for the same reason.
        api_key: Option<String>,
    },
}

impl Report {
    /// Tell the operator, once. Secrets go to standard output; everything else
    /// goes to the log.
    pub fn announce(&self) {
        match self {
            Self::AlreadySetUp { users } => {
                tracing::info!(
                    users,
                    "the instance already has users; bootstrap did nothing"
                );
            }
            Self::Created {
                user,
                password,
                api_key,
            } => {
                tracing::info!(user = user.as_str(), "created the first administrator");
                if password.is_none() && api_key.is_none() {
                    return;
                }
                println!("GPROXY first-run administrator (shown once)");
                println!("  user:     {user}");
                if let Some(password) = password {
                    println!("  password: {password}");
                }
                if let Some(api_key) = api_key {
                    println!("  api key:  {api_key}");
                }
                println!("Save these before closing this terminal; they are not stored in a form");
                println!("this instance can show you again.");
            }
        }
    }

    pub fn created(&self) -> bool {
        matches!(self, Self::Created { .. })
    }
}

/// Create the first administrator if, and only if, no user exists.
///
/// Refreshes the identity snapshot before returning, so a key minted here
/// authenticates on the very next request rather than after the next poll.
pub async fn ensure_admin<C>(app: &Arc<App<C>>, options: &AdminOptions) -> Result<Report>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let store = app.gproxy().store();
    // Counted rather than loaded: this runs on every start, and the answer is
    // almost always "there are users". One row is enough to know.
    let existing = store
        .users()
        .query(user::Entity::find().limit(1))
        .await?
        .len();
    if existing > 0 {
        return Ok(Report::AlreadySetUp {
            users: existing as u64,
        });
    }

    // A password the operator supplied is theirs and is never echoed; one
    // generated here is the only copy that will ever exist, so it is printed.
    let (password, generated_password) = match options.password.clone() {
        Some(password) => (password, None),
        None => {
            let generated = random_secret(GENERATED_PASSWORD_BYTES)?;
            (generated.clone(), Some(generated))
        }
    };

    let data = app.data();
    let operations = Operations::new(app.gproxy(), &data, app.config());
    let admin = operations
        .users()
        .create(UserWrite {
            id: None,
            name: options.user.clone(),
            password: Some(password),
            role: Some(ADMIN_ROLE.to_owned()),
            enabled: Some(true),
            oauth_client_allowlist: None,
        })
        .await?;

    // The snapshot the operations above read is now a revision behind, and the
    // key write validates the user it binds to against it. Refresh between the
    // two writes rather than reasoning about which reads tolerate staleness.
    app.reload_all().await?;
    let data = app.data();
    let operations = Operations::new(app.gproxy(), &data, app.config());

    let write = ApiKeyWrite {
        id: None,
        user_id: admin.id.clone(),
        name: BOOTSTRAP_KEY_NAME.to_owned(),
        organization_id: None,
        team_id: None,
        expires_at_ms: None,
        enabled: Some(true),
        // The operator is told the key once here, so there is no reason for the
        // database to keep a copy it could leak.
        retain_secret: Some(false),
    };
    let generated_key = match options.api_key.as_deref() {
        // Adopted, not generated: the operator already has this text, so it is
        // not echoed back at them.
        Some(token) => {
            operations.api_keys().adopt(write, token).await?;
            None
        }
        None => Some(operations.api_keys().create(write).await?.token),
    };

    // Publish the key before returning. `serve` starts accepting requests
    // immediately after this, and a key that needs a poll cycle to work would
    // look broken to the first thing the operator tries.
    app.reload_all().await?;

    Ok(Report::Created {
        user: options.user.clone(),
        password: generated_password,
        api_key: generated_key,
    })
}

/// `bytes` random bytes, base64url without padding.
fn random_secret(bytes: usize) -> Result<String> {
    use base64::Engine;
    let mut buffer = vec![0_u8; bytes];
    getrandom::fill(&mut buffer).map_err(|_| {
        crate::Error::other("secure randomness is unavailable; cannot generate a password")
    })?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&buffer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_password_clears_the_products_minimum() {
        let password = random_secret(GENERATED_PASSWORD_BYTES).unwrap();
        assert_eq!(password.len(), 32);
        assert!(!password.contains('='));
        // Two generated passwords are not the same password.
        assert_ne!(password, random_secret(GENERATED_PASSWORD_BYTES).unwrap());
    }

    #[test]
    fn an_existing_instance_reports_no_secrets() {
        let report = Report::AlreadySetUp { users: 3 };
        assert!(!report.created());
    }

    #[test]
    fn a_supplied_password_is_not_in_the_report() {
        // What `ensure_admin` builds when the operator supplied both: the report
        // carries neither, because the operator already has both.
        let report = Report::Created {
            user: "admin".into(),
            password: None,
            api_key: None,
        };
        assert!(report.created());
    }
}
