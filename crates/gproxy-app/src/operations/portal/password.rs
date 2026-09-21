//! The caller's own password.
//!
//! One operation, and the whole of it is a proof followed by a delegation.
//!
//! **The proof.** An account that has a password must present it; an account
//! that has none must not. The second half is not a nicety: an OAuth-only
//! account has nothing to prove, and demanding a `current` it never set would
//! make the one operation that could give it a password unreachable. Sending
//! one anyway is refused rather than ignored, because a client that thinks it
//! is re-authenticating and is not has misunderstood something worth being
//! told about.
//!
//! **The failure is `Forbidden`, not `Unauthorized`.** The rest of this crate
//! answers a bad credential with 401, and that is right for the credential a
//! request arrived on. This one is different: the session is perfectly valid,
//! and a 401 here would make a browser client discard it and send the user
//! back to the sign-in screen after a typo. The caller is authenticated and a
//! step inside the operation failed, which is what 403 says.
//!
//! **The delegation.** [`Users::set_password`](crate::operations::Users::set_password)
//! hashes and writes the new password and deletes the user's sessions in the
//! same revision commit. That includes *this* session: the portal signs the
//! user out everywhere, here included, and must re-authenticate afterwards.
//! Keeping the current one alive would need its id, and a session caller
//! carries none by design — a `Caller` is a person, not a row. Signing out
//! everywhere is also the behaviour a password change is for.

use gproxy_seaorm::BatchConnectionTrait;

use super::Portal;
use crate::{
    AppConfig, AppError, Caller, Result, auth::password as policy, dto::PortalPasswordChange,
    operations::Writer,
};

pub struct PortalPassword<'a, C> {
    portal: Portal<'a, C>,
}

impl<'a, C> PortalPassword<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>, config: &'a AppConfig, caller: Caller) -> Self {
        Self {
            portal: Portal::new(writer, config, caller),
        }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> PortalPassword<'_, C> {
    /// Set or replace the caller's password, and sign them out everywhere.
    pub async fn change(&self, change: PortalPasswordChange) -> Result<()> {
        self.portal.require_person("change the account password")?;
        let user_id = &self.portal.caller().user_id;
        let row = self
            .portal
            .writer()
            .store()
            .users()
            .get_many(std::slice::from_ref(user_id))
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| AppError::not_found("user", user_id))?;

        let supplied = change.current.as_deref().filter(|text| !text.is_empty());
        match (row.password_hash.as_deref(), supplied) {
            (Some(stored), Some(current)) => {
                if !policy::verify(current, stored) {
                    return Err(AppError::forbidden("the current password is incorrect"));
                }
            }
            (Some(_), None) => {
                return Err(AppError::invalid(
                    "currentPassword is required to change an existing password",
                ));
            }
            (None, Some(_)) => {
                return Err(AppError::invalid(
                    "this account has no password set; omit currentPassword",
                ));
            }
            (None, None) => {}
        }

        // `set_password` validates the new password, hashes it and ends every
        // session of this user in one revision commit.
        self.portal
            .operations()
            .users()
            .set_password(user_id, &change.new)
            .await?;
        Ok(())
    }
}
