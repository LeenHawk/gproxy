//! Turning what a login acquired into a credential row.
//!
//! One insert, through the same revision commit every configuration write
//! goes through, so a login is visible to peers by exactly the mechanism a
//! manual credential creation is. The scope is [`Scope::Credentials`] and not
//! the cheap credential-state path on purpose: a row that did not exist at the
//! last reload is not in the snapshot, and only a full reload can put it there.
//!
//! The secret is sealed with the configured codec before it reaches the
//! statement. Nothing between the channel and the database sees it in the
//! clear, and nothing sends it back to the caller.

use std::collections::HashSet;

use gproxy_channel::{BaseChannel, channel::AcquiredCredential};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::entity::upstream::{
    credential::{self, CredentialStatus},
    provider,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::Value;

use crate::{
    SdkError, SdkResult,
    dto::CredentialOwner,
    manage::{Scope, Writer},
};

/// The `auth_kind` of the two OAuth flows. It is what tells a channel which
/// shape to read the secret back in.
pub(crate) const OAUTH: &str = "oauth";
/// The `auth_kind` of a cookie exchange.
pub(crate) const COOKIE: &str = "cookie";

/// Insert the credential a login produced and return its id.
pub(crate) async fn credential<C>(
    writer: Writer<'_, C>,
    provider: &provider::Model,
    channel: &dyn BaseChannel,
    auth_kind: &str,
    label: Option<String>,
    owner: CredentialOwner,
    acquired: AcquiredCredential,
) -> SdkResult<String>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    if acquired.secret.is_null() {
        return Err(SdkError::invalid("the login produced no secret"));
    }
    let metadata = match acquired.metadata {
        Value::Null => Value::Object(Default::default()),
        value if value.is_object() => value,
        // A channel that answers with something else has a bug; storing it
        // would make every later read of the column guess.
        _ => {
            return Err(SdkError::invalid(
                "the login produced metadata that is not a JSON object",
            ));
        }
    };
    let id = crate::ids::random_id();
    let label = match trimmed(label) {
        Some(label) => Some(label),
        None => Some(default_label(writer, provider, channel, &metadata).await?),
    };
    let row = credential::ActiveModel {
        id: Set(id.clone()),
        provider_id: Set(provider.id.clone()),
        // Copied verbatim: this crate does not know what they mean.
        organization_id: Set(trimmed(owner.organization_id)),
        team_id: Set(trimmed(owner.team_id)),
        user_id: Set(trimmed(owner.user_id)),
        label: Set(label),
        auth_kind: Set(auth_kind.to_owned()),
        secret: Set(writer.core().secret_codec().seal(&id, &acquired.secret)?),
        // A fresh row starts at zero; a refresh is what moves it.
        version: Set(0),
        // The provider's own chain is right for a credential nobody configured
        // a profile for.
        connection_profile_id: Set(None),
        metadata: Set(metadata),
        expires_at_ms: Set(acquired.expires_at_ms),
        status: Set(CredentialStatus::Active),
        status_reason: Set(None),
        enabled: Set(true),
    };
    let statement = writer.store().credentials().insert_statement(row)?;
    writer
        .commit(
            vec![BatchStatement::Execute(statement)],
            &[Scope::Credentials(vec![id.clone()])],
        )
        .await?;
    Ok(id)
}

/// A name for a credential nobody named: the channel's display name, plus the
/// account the upstream identified if it identified one, made unique among the
/// provider's existing labels.
///
/// The point is that a console listing four logins on one provider can tell
/// them apart. It is not an identifier and nothing looks a credential up by it,
/// so "good enough, and never a duplicate" is the whole requirement.
async fn default_label<C>(
    writer: Writer<'_, C>,
    provider: &provider::Model,
    channel: &dyn BaseChannel,
    metadata: &Value,
) -> SdkResult<String>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let display_name = channel.descriptor().display_name;
    let base = match account(metadata) {
        Some(account) => format!("{display_name} {account}"),
        None => display_name.to_owned(),
    };
    let taken: HashSet<String> = writer
        .store()
        .credentials()
        .query(
            credential::Entity::find()
                .filter(credential::Column::ProviderId.eq(&provider.id))
                .filter(credential::Column::Label.is_not_null()),
        )
        .await?
        .into_iter()
        .filter_map(|row| row.label)
        .collect();
    if !taken.contains(&base) {
        return Ok(base);
    }
    // Bounded by the number of rows that can possibly be in the way, so a free
    // name is always among them.
    for suffix in 2..=taken.len().saturating_add(2) {
        let candidate = format!("{base} ({suffix})");
        if !taken.contains(&candidate) {
            return Ok(candidate);
        }
    }
    // Not reachable: more candidates were tried than there are labels. Falling
    // back rather than asserting keeps a completed login from being lost to a
    // naming detail.
    Ok(format!("{base} ({})", crate::ids::random_id()))
}

/// The account a login's metadata identifies, in the order of how much it says
/// about a person. `user_email` keeps its rate-limit tier, which is how a
/// console tells one plan's account from another's.
fn account(metadata: &Value) -> Option<String> {
    if let Some(email) = field(metadata, "user_email") {
        return Some(match field(metadata, "rate_limit_tier") {
            Some(tier) => format!("{email} {tier}"),
            None => email.to_owned(),
        });
    }
    [
        "email",
        "client_email",
        "chatgpt_account_id",
        "account_id",
        "account_uuid",
    ]
    .into_iter()
    .find_map(|key| field(metadata, key))
    .map(str::to_owned)
}

fn field<'a>(metadata: &'a Value, key: &str) -> Option<&'a str> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}
