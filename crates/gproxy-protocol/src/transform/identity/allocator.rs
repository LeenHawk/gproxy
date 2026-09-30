use super::{
    error::IdentityError,
    tool_alias,
    types::{DialectId, IdNamespace, IdentityRole, KnownIdPrefix, SourceIdentity},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

/// A target-specific syntax restriction, used only when supported by the
/// target contract. No vendor-wide character restriction is assumed by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdSyntax {
    AnyString,
    AsciiIdentifier,
}

/// Target validation used before preserving an upstream id or emitting a
/// generated id. Wire adapters can tighten this for a particular target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetIdPolicy {
    pub dialect: DialectId,
    pub max_len: Option<usize>,
    pub preserve_source_ids: bool,
    pub syntax: IdSyntax,
    pub required_prefix: Option<KnownIdPrefix>,
    pub allowed_generated_prefixes: Option<Vec<KnownIdPrefix>>,
    pub generated_prefix_overrides: Vec<(IdentityRole, KnownIdPrefix)>,
    /// Emit a converted tool call under the reversible alias of
    /// [`super::tool_alias`] whenever its upstream ID cannot go out as-is, so
    /// a later turn recovers the upstream side from the alias alone. Only a
    /// policy for IDs the client sends back sets this; IDs sent upstream are
    /// never decoded.
    #[serde(default)]
    pub reversible_tool_calls: bool,
}

impl TargetIdPolicy {
    pub fn new(dialect: DialectId) -> Self {
        Self {
            dialect,
            max_len: None,
            preserve_source_ids: true,
            syntax: IdSyntax::AnyString,
            required_prefix: None,
            allowed_generated_prefixes: None,
            generated_prefix_overrides: Vec::new(),
            reversible_tool_calls: false,
        }
    }

    pub fn with_reversible_tool_calls(mut self) -> Self {
        self.reversible_tool_calls = true;
        self
    }

    pub fn with_generated_prefixes(
        mut self,
        prefixes: impl IntoIterator<Item = KnownIdPrefix>,
    ) -> Self {
        self.allowed_generated_prefixes = Some(prefixes.into_iter().collect());
        self
    }

    pub fn with_generated_prefix(mut self, role: IdentityRole, prefix: KnownIdPrefix) -> Self {
        self.generated_prefix_overrides.push((role, prefix));
        self
    }

    pub fn with_max_len(mut self, max_len: usize) -> Self {
        self.max_len = Some(max_len);
        self
    }

    pub fn with_syntax(mut self, syntax: IdSyntax) -> Self {
        self.syntax = syntax;
        self
    }

    /// Only set this when the target's actual contract requires the prefix.
    /// Conventional generated prefixes alone do not justify this restriction.
    pub fn with_required_prefix(mut self, prefix: KnownIdPrefix) -> Self {
        self.required_prefix = Some(prefix);
        self
    }

    fn accepts_value(&self, value: &str) -> bool {
        valid_id(value, self.max_len)
            && (self.syntax == IdSyntax::AnyString
                || value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
            && self.required_prefix.is_none_or(|p| {
                value
                    .strip_prefix(p.as_str())
                    .is_some_and(|suffix| !suffix.is_empty())
            })
    }

    pub fn accepts_source(&self, value: &str) -> bool {
        self.preserve_source_ids && self.accepts_value(value)
    }

    pub fn accepts_generated(&self, prefix: KnownIdPrefix, value: &str) -> bool {
        self.allowed_generated_prefixes
            .as_ref()
            .is_none_or(|prefixes| prefixes.contains(&prefix))
            && self.accepts_value(value)
    }
}

fn valid_id(value: &str, max_len: Option<usize>) -> bool {
    !value.is_empty() && max_len.is_none_or(|max| value.len() <= max)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Locator {
    role: IdentityRole,
    dialect: DialectId,
    logical_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EmittedLookup {
    role: IdentityRole,
    id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SourceLookup {
    role: IdentityRole,
    dialect: DialectId,
    source_id: String,
}

#[derive(Debug, Clone)]
struct Entry {
    source: SourceIdentity,
    source_role: IdentityRole,
    emitted_id: String,
}

/// An immutable identity allocated within one invocation/stream flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityHandle {
    pub role: IdentityRole,
    pub source_role: IdentityRole,
    namespace: IdNamespace,
    pub source: SourceIdentity,
    pub emitted_id: String,
}

impl IdentityHandle {
    /// The id exposed on the target/client wire.
    pub fn client_id(&self) -> &str {
        &self.emitted_id
    }

    /// The source/native id, if the source supplied one (including a late
    /// source id attached after the client alias was emitted).
    pub fn source_id(&self) -> Option<&str> {
        self.source.source_id.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallResultLink {
    pub call_id: String,
    pub output_item_id: Option<String>,
}

/// Flow-local identity map. It must be created for each invocation; it does
/// not contain process-global state or random number generation.
#[derive(Debug, Clone)]
pub struct IdentityFlow {
    namespace: IdNamespace,
    used_ids: HashSet<EmittedLookup>,
    entries: HashMap<Locator, Entry>,
    by_source: HashMap<SourceLookup, Locator>,
    by_emitted: HashMap<EmittedLookup, Locator>,
    bound_target: Option<TargetIdPolicy>,
}

impl IdentityFlow {
    pub fn new(namespace: IdNamespace) -> Self {
        Self {
            namespace,
            used_ids: HashSet::new(),
            entries: HashMap::new(),
            by_source: HashMap::new(),
            by_emitted: HashMap::new(),
            bound_target: None,
        }
    }

    /// Reserve identities owned by other calls without inventing source associations.
    pub(crate) fn reserve_external_ids(
        &mut self,
        role: IdentityRole,
        ids: &BTreeSet<String>,
    ) -> Result<(), IdentityError> {
        if ids.iter().any(|id| {
            self.by_emitted.contains_key(&EmittedLookup {
                role,
                id: id.clone(),
            })
        }) {
            return Err(IdentityError::InvalidIdentity(
                "external identity already emitted by this call".into(),
            ));
        }
        for id in ids {
            self.used_ids.insert(EmittedLookup {
                role,
                id: id.clone(),
            });
        }
        Ok(())
    }

    /// Current associations, including source IDs learned after first emission.
    /// Iterator order is unspecified; persistence callers must sort their keys.
    pub fn handles(&self) -> impl Iterator<Item = IdentityHandle> + '_ {
        self.entries
            .iter()
            .map(|(locator, entry)| handle_from_entry(self.namespace, locator.role, entry))
    }

    pub fn namespace(&self) -> IdNamespace {
        self.namespace
    }

    /// Resolve an identity whose semantic role is unchanged across the edge.
    pub fn resolve_or_allocate(
        &mut self,
        role: IdentityRole,
        source: SourceIdentity,
        target: &TargetIdPolicy,
    ) -> Result<IdentityHandle, IdentityError> {
        self.resolve_as(role, role, source, target)
    }

    /// Re-identify across semantic roles. A call id is not reused as an output
    /// item id even when both destinations accept arbitrary string identifiers.
    pub fn resolve_as(
        &mut self,
        source_role: IdentityRole,
        role: IdentityRole,
        source: SourceIdentity,
        target: &TargetIdPolicy,
    ) -> Result<IdentityHandle, IdentityError> {
        self.resolve_inner(source_role, role, source, target, &BTreeSet::new(), false)
    }

    /// Resolve a legacy Chat `function_call`. It has no ID, and its client
    /// alias says so, so a later turn to a Chat upstream sends it back in the
    /// legacy form rather than as a modern call.
    pub fn resolve_legacy_chat_call(
        &mut self,
        source: SourceIdentity,
        target: &TargetIdPolicy,
    ) -> Result<IdentityHandle, IdentityError> {
        let role = IdentityRole::ToolCall;
        self.resolve_inner(role, role, source, target, &BTreeSet::new(), true)
    }

    /// Like [`Self::resolve_or_allocate`], but a new identity never takes an
    /// ID in `avoid`: one another call of the same client response owns.
    pub fn resolve_or_allocate_avoiding(
        &mut self,
        role: IdentityRole,
        source: SourceIdentity,
        target: &TargetIdPolicy,
        avoid: &BTreeSet<String>,
    ) -> Result<IdentityHandle, IdentityError> {
        self.resolve_inner(role, role, source, target, avoid, false)
    }

    fn resolve_inner(
        &mut self,
        source_role: IdentityRole,
        role: IdentityRole,
        mut source: SourceIdentity,
        target: &TargetIdPolicy,
        avoid: &BTreeSet<String>,
        legacy: bool,
    ) -> Result<IdentityHandle, IdentityError> {
        source.source_id = source.source_id.filter(|id| !id.is_empty());
        if let Some(bound) = &self.bound_target
            && bound != target
        {
            return Err(IdentityError::InvalidIdentity(
                "identity flow target policy changed after allocation".into(),
            ));
        }
        let locator = Locator {
            role,
            dialect: source.dialect,
            logical_index: source.logical_index,
        };
        if let Some(source_id) = source.source_id.as_deref() {
            let source_key = SourceLookup {
                role,
                dialect: source.dialect,
                source_id: source_id.to_owned(),
            };
            if let Some(previous) = self.by_source.get(&source_key)
                && previous.logical_index != source.logical_index
            {
                return Err(IdentityError::DuplicateSourceIdentity {
                    role: role.name(),
                    dialect: source.dialect.id().to_owned(),
                    source_id: source_id.to_owned(),
                    first_index: previous.logical_index,
                    duplicate_index: source.logical_index,
                });
            }
            if let Some(entry) = self.entries.get_mut(&locator) {
                if entry.source_role != source_role {
                    return Err(IdentityError::InvalidIdentity(
                        "source role changed at logical position".into(),
                    ));
                }
                if entry.source.source_id.as_deref() != Some(source_id) {
                    attach_source_inner(
                        &mut self.by_source,
                        &source_key,
                        &locator,
                        &mut entry.source,
                    )?;
                }
                if self.bound_target.is_none() {
                    self.bound_target = Some(target.clone());
                }
                return Ok(handle_from_entry(self.namespace, role, entry));
            }
        } else if role == IdentityRole::Resource {
            return Err(IdentityError::ResourceIdRequired);
        }

        if let Some(entry) = self.entries.get(&locator) {
            if entry.source_role != source_role {
                return Err(IdentityError::InvalidIdentity(
                    "source role changed at logical position".into(),
                ));
            }
            if self.bound_target.is_none() {
                self.bound_target = Some(target.clone());
            }
            return Ok(handle_from_entry(self.namespace, role, entry));
        }

        let emitted_id = match (source.source_id.as_deref(), role) {
            (_, IdentityRole::ToolCall)
                if source_role == role
                    && target.reversible_tool_calls
                    && source.dialect != target.dialect =>
            {
                self.tool_alias(&source, target, avoid, legacy)?
            }
            (Some(source_id), _)
                if source_role == role
                    && target.accepts_source(source_id)
                    && !avoid.contains(source_id)
                    && !self.used_ids.contains(&EmittedLookup {
                        role,
                        id: source_id.to_owned(),
                    }) =>
            {
                source_id.to_owned()
            }
            (Some(_), IdentityRole::Resource) => return Err(IdentityError::ResourceIdRequired),
            (_, IdentityRole::Resource) => return Err(IdentityError::ResourceIdRequired),
            _ => self.allocate_generated(role, source.dialect, source.logical_index, target)?,
        };
        let entry = Entry {
            source: source.clone(),
            source_role,
            emitted_id: emitted_id.clone(),
        };
        self.used_ids.insert(EmittedLookup {
            role,
            id: emitted_id.clone(),
        });
        self.by_emitted.insert(
            EmittedLookup {
                role,
                id: emitted_id.clone(),
            },
            locator.clone(),
        );
        if let Some(source_id) = source.source_id.as_deref() {
            self.by_source.insert(
                SourceLookup {
                    role,
                    dialect: source.dialect,
                    source_id: source_id.to_owned(),
                },
                locator.clone(),
            );
        }
        self.entries.insert(locator, entry.clone());
        self.bound_target = Some(target.clone());
        Ok(handle_from_entry(self.namespace, role, &entry))
    }

    /// A converted call goes out under its upstream ID when the client's
    /// dialect takes it and it cannot be mistaken for an alias; otherwise
    /// under the reversible alias that names it. No per-request namespace
    /// enters an alias for an upstream ID, so the same ID always gets the
    /// same alias. A repeated ID counts up until the alias is free.
    fn tool_alias(
        &self,
        source: &SourceIdentity,
        target: &TargetIdPolicy,
        avoid: &BTreeSet<String>,
        legacy: bool,
    ) -> Result<String, IdentityError> {
        let prefix = generated_prefix(IdentityRole::ToolCall, target)?;
        let free = |id: &str| {
            !avoid.contains(id)
                && !self.used_ids.contains(&EmittedLookup {
                    role: IdentityRole::ToolCall,
                    id: id.to_owned(),
                })
        };
        let Some(id) = source.source_id.as_deref() else {
            let namespace = self.namespace.hex();
            let alias = tool_alias::missing(
                prefix.as_str(),
                &namespace[..16],
                source.logical_index,
                legacy,
            );
            if !target.accepts_generated(prefix, &alias) {
                return Err(IdentityError::InvalidIdentity(
                    "target policy cannot accept generated identifier".into(),
                ));
            }
            if !free(&alias) {
                return Err(IdentityError::Collision(alias));
            }
            return Ok(alias);
        };
        if target.accepts_source(id)
            && tool_alias::accepts(target.dialect, id)
            && tool_alias::decode(id).is_none()
            && free(id)
        {
            return Ok(id.to_owned());
        }
        // Each taken alias rules out at most one count, so this always ends.
        let tries = self.used_ids.len() as u64 + avoid.len() as u64;
        for repeat in 0..=tries {
            let alias = tool_alias::upstream(prefix.as_str(), id, repeat);
            if !target.accepts_generated(prefix, &alias) {
                return Err(IdentityError::InvalidIdentity(
                    "target policy cannot accept a tool call alias".into(),
                ));
            }
            if free(&alias) {
                return Ok(alias);
            }
        }
        Err(IdentityError::Collision(id.to_owned()))
    }

    fn allocate_generated(
        &self,
        role: IdentityRole,
        source_dialect: DialectId,
        logical_index: u64,
        target: &TargetIdPolicy,
    ) -> Result<String, IdentityError> {
        let prefix = generated_prefix(role, target)?;
        let candidate = format!(
            "{}{}_{}_{}_{:x}",
            prefix.as_str(),
            self.namespace.hex(),
            dialect_key(source_dialect),
            role.allocation_key(),
            logical_index
        );
        if !target.accepts_generated(prefix, &candidate) {
            return Err(IdentityError::InvalidIdentity(
                "target policy cannot accept generated identifier".into(),
            ));
        }
        if self.used_ids.contains(&EmittedLookup {
            role,
            id: candidate.clone(),
        }) {
            // Do not resolve a collision using arrival order: fail deterministically.
            return Err(IdentityError::Collision(candidate));
        }
        Ok(candidate)
    }

    /// Attach a source id that arrived after the emitted alias was sent. The
    /// returned alias is always the original immutable alias.
    pub fn attach_source(
        &mut self,
        handle: &IdentityHandle,
        source_id: impl Into<String>,
    ) -> Result<IdentityHandle, IdentityError> {
        let source_id = source_id.into();
        if source_id.is_empty() {
            return Err(IdentityError::InvalidIdentity(
                "late source id is invalid".into(),
            ));
        }
        let locator = self
            .by_emitted
            .get(&EmittedLookup {
                role: handle.role,
                id: handle.emitted_id.clone(),
            })
            .cloned()
            .ok_or_else(|| IdentityError::AmbiguousSourceIdentity("unknown emitted id".into()))?;
        let entry = self
            .entries
            .get_mut(&locator)
            .ok_or_else(|| IdentityError::AmbiguousSourceIdentity("missing flow entry".into()))?;
        let source_key = SourceLookup {
            role: locator.role,
            dialect: locator.dialect,
            source_id: source_id.clone(),
        };
        attach_source_inner(
            &mut self.by_source,
            &source_key,
            &locator,
            &mut entry.source,
        )?;
        Ok(handle_from_entry(self.namespace, locator.role, entry))
    }

    pub fn lookup_source(
        &self,
        source: &SourceIdentity,
        role: IdentityRole,
    ) -> Option<IdentityHandle> {
        let source_id = source.source_id.as_deref()?;
        let key = SourceLookup {
            role,
            dialect: source.dialect,
            source_id: source_id.to_owned(),
        };
        let locator = self.by_source.get(&key)?;
        self.entries
            .get(locator)
            .map(|entry| handle_from_entry(self.namespace, role, entry))
    }

    /// Look up an entry when a stream event has no source id yet.
    pub fn lookup_logical(
        &self,
        role: IdentityRole,
        dialect: &DialectId,
        logical_index: u64,
    ) -> Option<IdentityHandle> {
        let locator = Locator {
            role,
            dialect: *dialect,
            logical_index,
        };
        self.entries
            .get(&locator)
            .map(|entry| handle_from_entry(self.namespace, role, entry))
    }

    pub fn lookup_emitted(&self, emitted_id: &str) -> Option<IdentityHandle> {
        let mut matches = self
            .by_emitted
            .iter()
            .filter(|(key, _)| key.id == emitted_id);
        let (_, locator) = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        self.entries
            .get(locator)
            .map(|entry| handle_from_entry(self.namespace, locator.role, entry))
    }

    pub fn lookup_emitted_as(
        &self,
        role: IdentityRole,
        emitted_id: &str,
    ) -> Option<IdentityHandle> {
        let locator = self.by_emitted.get(&EmittedLookup {
            role,
            id: emitted_id.to_owned(),
        })?;
        self.entries
            .get(locator)
            .map(|entry| handle_from_entry(self.namespace, role, entry))
    }

    pub fn lookup_client_id(&self, client_id: &str) -> Option<IdentityHandle> {
        self.lookup_emitted(client_id)
    }

    pub fn lookup_native(
        &self,
        source: &SourceIdentity,
        role: IdentityRole,
    ) -> Option<IdentityHandle> {
        self.lookup_source(source, role)
    }

    pub fn link_call_result(
        &self,
        call: &IdentityHandle,
        result: Option<&IdentityHandle>,
    ) -> Result<CallResultLink, IdentityError> {
        if call.role != IdentityRole::ToolCall {
            return Err(IdentityError::InvalidIdentity(
                "call-result link requires a ToolCall".into(),
            ));
        }
        if result.is_some_and(|result| {
            !matches!(
                result.role,
                IdentityRole::OutputItem(
                    super::types::OutputItemKind::FunctionCallOutput
                        | super::types::OutputItemKind::CustomToolCallOutput
                )
            )
        }) {
            return Err(IdentityError::InvalidIdentity(
                "call-result link requires a function/custom result item".into(),
            ));
        }
        Ok(CallResultLink {
            call_id: call.emitted_id.clone(),
            output_item_id: result.map(|result| result.emitted_id.clone()),
        })
    }
}

fn attach_source_inner(
    by_source: &mut HashMap<SourceLookup, Locator>,
    key: &SourceLookup,
    locator: &Locator,
    source: &mut SourceIdentity,
) -> Result<(), IdentityError> {
    if let Some(previous) = by_source.get(key)
        && previous != locator
    {
        return Err(IdentityError::DuplicateSourceIdentity {
            role: locator.role.name(),
            dialect: locator.dialect.id().to_owned(),
            source_id: key.source_id.clone(),
            first_index: previous.logical_index,
            duplicate_index: locator.logical_index,
        });
    }
    if let Some(existing) = source.source_id.as_deref()
        && existing != key.source_id
    {
        return Err(IdentityError::AmbiguousSourceIdentity(format!(
            "entry already has source id {existing:?}"
        )));
    }
    source.source_id = Some(key.source_id.clone());
    by_source.insert(key.clone(), locator.clone());
    Ok(())
}

fn handle_from_entry(namespace: IdNamespace, role: IdentityRole, entry: &Entry) -> IdentityHandle {
    IdentityHandle {
        role,
        source_role: entry.source_role,
        namespace,
        source: entry.source.clone(),
        emitted_id: entry.emitted_id.clone(),
    }
}

fn generated_prefix(
    role: IdentityRole,
    target: &TargetIdPolicy,
) -> Result<KnownIdPrefix, IdentityError> {
    target
        .generated_prefix_overrides
        .iter()
        .rev()
        .find_map(|(r, p)| (*r == role).then_some(*p))
        .or_else(|| role.generated_prefix())
        .ok_or(IdentityError::NoGeneratedPrefix)
}

fn dialect_key(dialect: DialectId) -> &'static str {
    match dialect {
        DialectId::OpenAi => "o",
        DialectId::Claude => "c",
        DialectId::Gemini => "g",
        DialectId::OpenAiChat => "h",
        DialectId::OpenAiResponsesWebSocket => "w",
    }
}
