//! The management audit trail: who did what, to which entity, with what
//! outcome.
//!
//! Three decisions shape this module.
//!
//! **An audit row is written outside the revision batch.** It is history, not
//! configuration: nothing reads it to serve a request, no peer reloads because
//! of it, and `AppData` does not contain it. Putting it in the same
//! transaction as the operation would mean a failing insert — a trail table
//! that has run out of space, a detail that does not encode — rolls back the
//! operation it was only describing. It would also make it impossible to audit
//! a *rejected* operation, which is the row an investigation actually wants,
//! because that operation has no transaction to join. So the trail is written
//! after the fact, and a trail write that fails is logged rather than
//! propagated.
//!
//! **`detail` is redacted before it is built, not before it is read.** There
//! is no "redact on display" mode: a secret that reaches the column has
//! already leaked into every backup and every replica. [`AuditEntry::redacted`]
//! is the only supported way to put caller-supplied JSON into an entry.
//!
//! **The actor columns have no foreign keys**, by the entity's own design: the
//! user, the key and the target may all be deleted later, and the trail has to
//! survive exactly those deletions.

use crate::{
    AppError, Result,
    dto::{AuditEventDto, AuditQuery, Page},
};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{Store, entity::identity::audit_event};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde_json::{Map, Value};

/// Field names whose value is replaced by [`REDACTED`] wherever they appear in
/// an audit detail, at any depth.
///
/// Matching ignores case, underscores and dashes, so `apiKey`, `api_key`,
/// `API-KEY` and `apikey` are one entry. The list is names, not values: a
/// heuristic that tried to recognise a secret by its shape would miss a short
/// one and redact a model id that happened to look random.
///
/// It is deliberately over-broad. A redacted field that did not need to be
/// costs an operator one question; a field that needed it and was not is
/// permanent.
pub const REDACTED_FIELDS: [&str; 26] = [
    "accesstoken",
    "apikey",
    "authorization",
    "clientsecret",
    "code",
    "codeverifier",
    "cookie",
    "credentials",
    "currentpassword",
    "idtoken",
    "key",
    "keyhash",
    "masterkey",
    "newpassword",
    "oldpassword",
    "password",
    "passwordhash",
    "privatekey",
    "refreshtoken",
    "secret",
    "sessiontoken",
    "setcookie",
    "token",
    "tokenhash",
    "verifier",
    "xapikey",
];

/// What a redacted value is replaced with. A fixed string rather than removal,
/// so a reader can tell "this operation carried a password" from "this
/// operation did not".
pub const REDACTED: &str = "[redacted]";

/// One row of the trail, before it has an id or a clock.
///
/// Construct it with [`AuditEntry::new`] and fill in the rest; `detail` should
/// only ever be set from [`AuditEntry::redacted`].
#[derive(Clone, Debug, Default)]
pub struct AuditEntry {
    /// The acting user, when the operation arrived on a console or portal
    /// session or on a key whose owner is known.
    pub actor_user_id: Option<String>,
    /// The acting API key, when the operation arrived on a programmatic
    /// caller.
    pub actor_api_key_id: Option<String>,
    /// The client address as the host's trusted-proxy policy resolved it.
    /// This crate never parses a forwarding header itself.
    pub source_ip: Option<String>,
    /// The operation's name, e.g. `api_keys.rotate`.
    pub action: String,
    /// The target family and id, when the action names one.
    pub entity_kind: Option<String>,
    pub entity_id: Option<String>,
    /// [`audit_event::OUTCOME_OK`] or [`audit_event::OUTCOME_ERROR`].
    pub outcome: String,
    /// A redacted summary. Its shape belongs to the operation that wrote it.
    pub detail: Value,
}

impl AuditEntry {
    /// A successful operation with no detail yet.
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            actor_user_id: None,
            actor_api_key_id: None,
            source_ip: None,
            action: action.into(),
            entity_kind: None,
            entity_id: None,
            outcome: audit_event::OUTCOME_OK.to_owned(),
            detail: Value::Null,
        }
    }

    /// Fill the actor columns from an authenticated caller.
    ///
    /// A session caller has no key, so `actor_api_key_id` stays empty; a key
    /// caller records both, because "which key" and "whose key" are different
    /// questions during an investigation.
    pub fn by(mut self, caller: &crate::Caller) -> Self {
        self.actor_user_id = Some(caller.user_id.clone());
        self.actor_api_key_id = caller.api_key_id.clone();
        self
    }

    pub fn source_ip(mut self, address: Option<impl Into<String>>) -> Self {
        self.source_ip = address.map(Into::into);
        self
    }

    pub fn entity(mut self, kind: impl Into<String>, id: impl Into<String>) -> Self {
        self.entity_kind = Some(kind.into());
        self.entity_id = Some(id.into());
        self
    }

    /// Mark the entry as recording a refused operation, with the error's
    /// stable code as the reason.
    ///
    /// The code, not the message: a message can carry a value the caller sent,
    /// and a caller that sends its password into a field the validator rejects
    /// would otherwise see it quoted back into the trail.
    pub fn failed(mut self, error: &AppError) -> Self {
        self.outcome = audit_event::OUTCOME_ERROR.to_owned();
        if let Some(object) = self.detail.as_object_mut() {
            object.insert("error".into(), Value::String(error.code().into()));
        } else {
            self.detail = serde_json::json!({ "error": error.code() });
        }
        self
    }

    /// Attach an already-redacted detail. Use [`AuditEntry::redacted`] to
    /// produce one.
    pub fn detail(mut self, detail: Value) -> Self {
        self.detail = detail;
        self
    }

    /// `value` with every field named in [`REDACTED_FIELDS`] replaced by
    /// [`REDACTED`], recursively through objects and arrays.
    ///
    /// The whole value is walked, not just its top level, because an operation
    /// body nests: a user write carries `{"user": {"password": …}}` and a
    /// batch carries a list of them. Non-object leaves are returned unchanged;
    /// there is nothing to name in a bare string, which is why a caller should
    /// pass the request body rather than one field of it.
    pub fn redacted(value: Value) -> Value {
        match value {
            Value::Object(fields) => Value::Object(
                fields
                    .into_iter()
                    .map(|(name, value)| {
                        if is_sensitive(&name) {
                            (name, Value::String(REDACTED.to_owned()))
                        } else {
                            (name, Self::redacted(value))
                        }
                    })
                    .collect::<Map<_, _>>(),
            ),
            Value::Array(items) => {
                Value::Array(items.into_iter().map(Self::redacted).collect::<Vec<_>>())
            }
            other => other,
        }
    }
}

/// Whether a field name is one of the redacted ones, ignoring case and the
/// separators the same name is spelled with in JSON, SQL and Rust.
fn is_sensitive(name: &str) -> bool {
    let normalized: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    REDACTED_FIELDS.contains(&normalized.as_str())
}

/// Reading and writing the trail. Holds only the store: nothing here consults
/// the identity snapshot, because a historical row must not change meaning
/// when the row it names is edited.
pub struct Audit<'a, C> {
    store: &'a Store<C>,
}

impl<'a, C> Audit<'a, C> {
    pub fn new(store: &'a Store<C>) -> Self {
        Self { store }
    }
}

impl<C: BatchConnectionTrait> Audit<'_, C> {
    /// Append one row, and answer its id.
    ///
    /// Outside the revision batch, and therefore after the operation it
    /// describes has already committed or already failed.
    pub async fn record(&self, entry: AuditEntry) -> Result<String> {
        self.record_at(entry, crate::now_ms()).await
    }

    /// [`Audit::record`] against a stated clock, so a test can page a trail
    /// whose order it chose.
    pub async fn record_at(&self, entry: AuditEntry, now_ms: i64) -> Result<String> {
        let id = crate::operations::random_id()?;
        self.store
            .audit_events()
            .create_many(vec![audit_event::ActiveModel {
                id: Set(id.clone()),
                actor_user_id: Set(entry.actor_user_id),
                actor_api_key_id: Set(entry.actor_api_key_id),
                source_ip: Set(entry.source_ip),
                action: Set(entry.action),
                entity_kind: Set(entry.entity_kind),
                entity_id: Set(entry.entity_id),
                outcome: Set(entry.outcome),
                detail: Set(entry.detail),
                created_at_ms: Set(now_ms),
            }])
            .await?;
        Ok(id)
    }

    /// Append one row, swallowing a failure into a warning.
    ///
    /// For the host middleware, which records every operation: a trail that
    /// cannot be written must not turn a successful operation into a 500 the
    /// caller will retry, producing the side effect twice.
    pub async fn try_record(&self, entry: AuditEntry) {
        let action = entry.action.clone();
        if let Err(error) = self.record(entry).await {
            tracing::warn!(%error, %action, "audit event could not be recorded");
        }
    }

    /// One page of the trail, newest first.
    ///
    /// The order is `created_at_ms DESC`, then the primary key ascending,
    /// which the repository appends: two rows written in the same millisecond
    /// would otherwise page non-deterministically and could repeat or skip
    /// across page boundaries.
    pub async fn query(&self, query: AuditQuery) -> Result<Page<AuditEventDto>> {
        let (offset, limit) = query.bounds();
        let mut select =
            audit_event::Entity::find().order_by_desc(audit_event::Column::CreatedAtMs);
        if let Some(actor) = trimmed(query.actor_user_id) {
            select = select.filter(audit_event::Column::ActorUserId.eq(actor));
        }
        if let Some(actor) = trimmed(query.actor_api_key_id) {
            select = select.filter(audit_event::Column::ActorApiKeyId.eq(actor));
        }
        if let Some(action) = trimmed(query.action) {
            select = select.filter(audit_event::Column::Action.contains(&action));
        }
        if let Some(kind) = trimmed(query.entity_kind) {
            select = select.filter(audit_event::Column::EntityKind.eq(kind));
        }
        if let Some(id) = trimmed(query.entity_id) {
            select = select.filter(audit_event::Column::EntityId.eq(id));
        }
        if let Some(outcome) = trimmed(query.outcome) {
            select = select.filter(audit_event::Column::Outcome.eq(outcome));
        }
        if let Some(since) = query.since_ms {
            select = select.filter(audit_event::Column::CreatedAtMs.gte(since));
        }
        if let Some(until) = query.until_ms {
            select = select.filter(audit_event::Column::CreatedAtMs.lt(until));
        }
        let page = self
            .store
            .audit_events()
            .page(select, offset, limit)
            .await?;
        Ok(Page::convert(page, AuditEventDto::from))
    }
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_spelling_of_a_sensitive_name_is_redacted() {
        for name in [
            "password",
            "Password",
            "new_password",
            "newPassword",
            "API-KEY",
            "apiKey",
            "key_hash",
            "tokenHash",
            "client_secret",
            "refresh_token",
            "codeVerifier",
            "set-cookie",
        ] {
            let redacted = AuditEntry::redacted(json!({ name: "s3cret" }));
            assert_eq!(redacted[name], json!(REDACTED), "{name} survived");
        }
    }

    #[test]
    fn redaction_reaches_every_depth() {
        let redacted = AuditEntry::redacted(json!({
            "user": { "name": "alice", "password": "hunter2" },
            "items": [{ "token": "t" }, { "label": "keep" }],
        }));
        assert_eq!(redacted["user"]["name"], json!("alice"));
        assert_eq!(redacted["user"]["password"], json!(REDACTED));
        assert_eq!(redacted["items"][0]["token"], json!(REDACTED));
        assert_eq!(redacted["items"][1]["label"], json!("keep"));
    }

    #[test]
    fn a_redacted_field_is_replaced_rather_than_removed() {
        // A reader must be able to tell "carried a password" from "did not".
        let redacted = AuditEntry::redacted(json!({ "password": "hunter2" }));
        assert!(redacted.as_object().unwrap().contains_key("password"));
        assert_eq!(redacted["password"], json!(REDACTED));
        // And a nested secret is not read as a value to walk into.
        let redacted = AuditEntry::redacted(json!({ "secret": { "inner": "x" } }));
        assert_eq!(redacted["secret"], json!(REDACTED));
    }

    #[test]
    fn a_name_that_merely_contains_a_sensitive_word_is_kept() {
        // The list is exact names after normalization, not substrings: a
        // `keyboardLayout` or a `passwordPolicy` is configuration, not a
        // secret, and blanking it would make the trail useless.
        let redacted = AuditEntry::redacted(json!({
            "keyboardLayout": "dvorak",
            "passwordPolicy": "long",
            "tokenizerId": "t1",
        }));
        assert_eq!(redacted["keyboardLayout"], json!("dvorak"));
        assert_eq!(redacted["passwordPolicy"], json!("long"));
        assert_eq!(redacted["tokenizerId"], json!("t1"));
    }

    #[test]
    fn a_failure_records_the_code_and_never_the_message() {
        let error = AppError::invalid("password `hunter2` is too short");
        let entry = AuditEntry::new("users.create")
            .detail(AuditEntry::redacted(json!({ "name": "alice" })))
            .failed(&error);
        assert_eq!(entry.outcome, audit_event::OUTCOME_ERROR);
        assert_eq!(entry.detail["error"], json!("invalid_request"));
        assert_eq!(entry.detail["name"], json!("alice"));
        assert!(!entry.detail.to_string().contains("hunter2"));
    }
}
