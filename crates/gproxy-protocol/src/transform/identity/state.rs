use super::{
    error::IdentityError,
    types::{DialectId, IdentityRole},
};
use crate::capability::{CasResult, StateEntry, StateStore, StateWrite, Version};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

const STATE_SCHEMA: u16 = 2;

/// Target semantics are persisted with an identity record. A record from a
/// different model or target dialect cannot be restored into a new flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityTarget {
    pub model: String,
    pub dialect: DialectId,
    /// Host-owned upstream/principal origin for signature binding. It is
    /// intentionally separate from the wire dialect.
    pub origin: Option<String>,
}

impl IdentityTarget {
    pub fn new(model: impl Into<String>, dialect: DialectId) -> Result<Self, IdentityError> {
        let model = model.into();
        if model.trim().is_empty() {
            return Err(IdentityError::InvalidIdentity(
                "target model is invalid".into(),
            ));
        }
        Ok(Self {
            model,
            dialect,
            origin: None,
        })
    }

    pub fn with_origin(mut self, origin: impl Into<String>) -> Result<Self, IdentityError> {
        let origin = origin.into();
        if origin.trim().is_empty() {
            return Err(IdentityError::InvalidIdentity(
                "identity origin is empty".into(),
            ));
        }
        self.origin = Some(origin);
        Ok(self)
    }
}

/// The exact declared upstream field that owns an opaque value. A matching
/// origin/model does not permit moving bytes between these fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpaqueField {
    ClaudeThinkingSignature,
    ClaudeRedactedThinkingData,
    GeminiPartThoughtSignature,
    ResponsesReasoningEncryptedContent,
}

impl OpaqueField {
    pub fn dialect(self) -> DialectId {
        match self {
            Self::ClaudeThinkingSignature | Self::ClaudeRedactedThinkingData => DialectId::Claude,
            Self::GeminiPartThoughtSignature => DialectId::Gemini,
            Self::ResponsesReasoningEncryptedContent => DialectId::OpenAi,
        }
    }
}

/// A declared opaque signature can be restored only for its original origin,
/// model and field. It is intentionally just a small token, never a wire DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpaqueSignature {
    pub field: OpaqueField,
    pub value: String,
    pub origin: String,
    pub model: String,
}

impl OpaqueSignature {
    pub fn new(
        field: OpaqueField,
        value: impl Into<String>,
        origin: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, IdentityError> {
        let value = value.into();
        let origin = origin.into();
        let model = model.into();
        if value.is_empty() || origin.trim().is_empty() || model.trim().is_empty() {
            return Err(IdentityError::InvalidIdentity(
                "opaque signature declaration is invalid".into(),
            ));
        }
        Ok(Self {
            field,
            value,
            origin,
            model,
        })
    }
}

/// The only identity facts allowed to cross a request boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityStateRecord {
    pub schema: u16,
    pub role: IdentityRole,
    pub target: IdentityTarget,
    pub original_item_id: Option<String>,
    pub original_call_id: Option<String>,
    pub client_item_id: Option<String>,
    pub client_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub response_id: Option<String>,
    pub conversation_id: Option<String>,
    pub opaque_signature: Option<OpaqueSignature>,
}

impl IdentityStateRecord {
    pub fn new(role: IdentityRole, target: IdentityTarget) -> Self {
        Self {
            schema: STATE_SCHEMA,
            role,
            target,
            original_item_id: None,
            original_call_id: None,
            client_item_id: None,
            client_call_id: None,
            tool_name: None,
            response_id: None,
            conversation_id: None,
            opaque_signature: None,
        }
    }

    pub fn validate_for(&self, expected: &IdentityTarget) -> Result<(), IdentityError> {
        if self.target.model.trim().is_empty()
            || self
                .target
                .origin
                .as_ref()
                .is_some_and(|origin| origin.trim().is_empty())
        {
            return Err(IdentityError::InvalidIdentity(
                "invalid state target binding".into(),
            ));
        }
        if self.schema != STATE_SCHEMA {
            return Err(IdentityError::InvalidIdentity(
                "unsupported identity state schema".into(),
            ));
        }
        if &self.target != expected {
            return Err(IdentityError::InvalidIdentity(format!(
                "state target {} / {} does not match {} / {}",
                self.target.dialect.id(),
                self.target.model,
                expected.dialect.id(),
                expected.model
            )));
        }
        for value in [
            &self.original_item_id,
            &self.original_call_id,
            &self.client_item_id,
            &self.client_call_id,
            &self.response_id,
            &self.conversation_id,
        ]
        .into_iter()
        .flatten()
        {
            if value.is_empty() {
                return Err(IdentityError::InvalidIdentity(
                    "identity state id is invalid".into(),
                ));
            }
        }
        if let Some(name) = &self.tool_name
            && name.is_empty()
        {
            return Err(IdentityError::InvalidIdentity(
                "tool name is invalid".into(),
            ));
        }
        if let Some(signature) = &self.opaque_signature
            && (signature.value.is_empty()
                || signature.origin.trim().is_empty()
                || signature.model.trim().is_empty()
                || self.target.origin.as_deref() != Some(signature.origin.as_str())
                || signature.model != self.target.model
                || signature.field.dialect() != self.target.dialect)
        {
            return Err(IdentityError::InvalidIdentity(
                "opaque signature origin/model/field does not match its target".into(),
            ));
        }
        Ok(())
    }
}

/// Fields that may be learned after the first stream event. Existing values
/// are immutable; a contradictory late fact is rejected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LateIdentityFacts {
    pub original_item_id: Option<String>,
    pub original_call_id: Option<String>,
    pub client_item_id: Option<String>,
    pub client_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub response_id: Option<String>,
    pub conversation_id: Option<String>,
    pub opaque_signature: Option<OpaqueSignature>,
}

impl LateIdentityFacts {
    fn apply(self, record: &mut IdentityStateRecord) -> Result<(), IdentityError> {
        merge(
            &mut record.original_item_id,
            self.original_item_id,
            "original_item_id",
        )?;
        merge(
            &mut record.original_call_id,
            self.original_call_id,
            "original_call_id",
        )?;
        merge(
            &mut record.client_item_id,
            self.client_item_id,
            "client_item_id",
        )?;
        merge(
            &mut record.client_call_id,
            self.client_call_id,
            "client_call_id",
        )?;
        merge(&mut record.tool_name, self.tool_name, "tool_name")?;
        merge(&mut record.response_id, self.response_id, "response_id")?;
        merge(
            &mut record.conversation_id,
            self.conversation_id,
            "conversation_id",
        )?;
        if let Some(signature) = self.opaque_signature {
            if let Some(existing) = &record.opaque_signature {
                if existing != &signature {
                    return Err(IdentityError::InvalidIdentity(
                        "opaque signature changed after emission".into(),
                    ));
                }
            } else {
                record.opaque_signature = Some(signature);
            }
        }
        Ok(())
    }
}

fn merge(
    slot: &mut Option<String>,
    incoming: Option<String>,
    field: &str,
) -> Result<(), IdentityError> {
    let Some(incoming) = incoming else {
        return Ok(());
    };
    if let Some(existing) = slot {
        if existing != &incoming {
            return Err(IdentityError::InvalidIdentity(format!(
                "{field} changed after emission"
            )));
        }
    } else {
        *slot = Some(incoming);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityStateSnapshot {
    pub record: IdentityStateRecord,
    pub version: Version,
    pub expires_at: Option<SystemTime>,
}

/// Adapter-neutral persistence helper over the protocol's scoped CAS store.
pub struct IdentityStateStore<S> {
    store: S,
}

impl<S> IdentityStateStore<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }
    pub fn inner(&self) -> &S {
        &self.store
    }
}

impl<S: StateStore> IdentityStateStore<S> {
    /// `scope` must bind tenant, upstream and principal; keys must also bind
    /// the conversation/invocation. A wire dialect is not an upstream identity.
    pub async fn read(
        &self,
        scope: &S::Scope,
        key: &str,
        target: &IdentityTarget,
    ) -> Result<IdentityStateSnapshot, IdentityError> {
        let Some(entry) = self
            .store
            .get(scope, key)
            .await
            .map_err(IdentityError::StateStore)?
        else {
            return Err(IdentityError::MissingState);
        };
        decode_entry(entry, target)
    }

    /// Save a record with compare-and-exchange. `expected = None` creates a
    /// fresh scope key; callers should perform this before exposing an alias.
    pub async fn save(
        &self,
        scope: &S::Scope,
        key: &str,
        expected: Option<Version>,
        record: &IdentityStateRecord,
        expires_at: Option<SystemTime>,
    ) -> Result<Version, IdentityError> {
        record.validate_for(&record.target)?;
        let payload = serde_json::to_vec(record).map_err(IdentityError::StateEncoding)?;
        let result = self
            .store
            .compare_exchange(
                scope,
                key,
                expected,
                Some(StateWrite {
                    payload: Bytes::from(payload),
                    expires_at,
                }),
            )
            .await
            .map_err(IdentityError::StateStore)?;
        match result {
            CasResult::Applied(Some(version)) => Ok(version),
            CasResult::Applied(None) | CasResult::Conflict => Err(IdentityError::Conflict),
        }
    }

    /// CAS update for source ids/signatures that arrive after an alias was
    /// emitted. Existing aliases remain untouched in the record.
    pub async fn update_late(
        &self,
        scope: &S::Scope,
        key: &str,
        expected: Version,
        target: &IdentityTarget,
        facts: LateIdentityFacts,
    ) -> Result<IdentityStateSnapshot, IdentityError> {
        let current = self.read(scope, key, target).await?;
        if current.version != expected {
            return Err(IdentityError::Conflict);
        }
        let mut record = current.record;
        facts.apply(&mut record)?;
        record.validate_for(target)?;
        let version = self
            .save(scope, key, Some(expected), &record, current.expires_at)
            .await?;
        Ok(IdentityStateSnapshot {
            record,
            version,
            expires_at: current.expires_at,
        })
    }
}

fn decode_entry(
    entry: StateEntry,
    target: &IdentityTarget,
) -> Result<IdentityStateSnapshot, IdentityError> {
    // Expiration is enforced atomically by StateStore::get. Do not introduce
    // a second clock (or native-only SystemTime::now) at the protocol layer.
    let record: IdentityStateRecord =
        serde_json::from_slice(&entry.payload).map_err(IdentityError::StateEncoding)?;
    record.validate_for(target)?;
    Ok(IdentityStateSnapshot {
        record,
        version: entry.version,
        expires_at: entry.expires_at,
    })
}
