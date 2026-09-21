use super::super::{GenerationStateAccess, state::SavedIdentities};
use super::*;
use crate::{
    WireResponse,
    capability::{CasResult, StateStore, StateWrite},
    transform::identity::{IdentityTarget, TargetIdPolicy},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::SystemTime};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Raw {
    status: u16,
    headers: Vec<(String, Vec<u8>)>,
    body: Vec<u8>,
}

impl Raw {
    pub fn from_wire(v: &WireResponse<bytes::Bytes>) -> Self {
        Self {
            status: v.status.as_u16(),
            headers: v
                .headers
                .iter()
                .map(|(k, v)| (k.as_str().into(), v.as_bytes().to_vec()))
                .collect(),
            body: v.body.to_vec(),
        }
    }
    pub fn wire(&self) -> Result<WireResponse<bytes::Bytes>, TransformError> {
        let mut headers = http::HeaderMap::new();
        for (name, value) in &self.headers {
            headers.append(
                http::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|e| conflict(e.to_string()))?,
                http::HeaderValue::from_bytes(value).map_err(|e| conflict(e.to_string()))?,
            );
        }
        Ok(WireResponse {
            status: http::StatusCode::from_u16(self.status).map_err(|e| conflict(e.to_string()))?,
            headers,
            body: self.body.clone().into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ChildBinding {
    pub request: Vec<u8>,
    pub namespaces: (IdNamespace, IdNamespace),
    pub policies: (TargetIdPolicy, TargetIdPolicy),
}

type SavedEndpoint = (String, Option<String>, Vec<(String, Vec<u8>)>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Binding {
    pub id: String,
    pub original: Vec<u8>,
    pub target: IdentityTarget,
    pub conversation: String,
    pub expires_at: SystemTime,
    pub endpoint: SavedEndpoint,
    pub children: Vec<ChildBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(super) struct SavedChild {
    pub started: bool,
    pub raw: Option<Raw>,
    pub identities: BTreeMap<String, (Vec<u8>, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Journal {
    pub schema: u16,
    pub binding: Binding,
    pub children: Vec<SavedChild>,
    pub group_identities: BTreeMap<String, (Vec<u8>, Vec<u8>)>,
    pub exposed: Option<Vec<u8>>,
}

impl Journal {
    pub fn key(&self) -> String {
        format!(
            "generate-fanout:{}:{}:{}:{}",
            self.binding.conversation.len(),
            self.binding.conversation,
            self.binding.id.len(),
            self.binding.id
        )
    }
}

pub(super) fn pack(v: &SavedIdentities) -> BTreeMap<String, (Vec<u8>, Vec<u8>)> {
    v.iter()
        .map(|(k, (version, bytes))| (k.clone(), (version.as_bytes().to_vec(), bytes.clone())))
        .collect()
}

fn unpack(v: &BTreeMap<String, (Vec<u8>, Vec<u8>)>) -> SavedIdentities {
    v.iter()
        .map(|(k, (version, bytes))| {
            (
                k.clone(),
                (Version::from_bytes(version.clone()), bytes.clone()),
            )
        })
        .collect()
}

pub(super) async fn save<S: StateStore, N>(
    state: &GenerationStateAccess<'_, S>,
    limits: CodecLimits,
    progress: &mut FanoutProgress<N>,
) -> Result<(), TransformError> {
    let value = progress.journal.as_ref().expect("journal initialized");
    let bytes = encode(value, limits)?;
    if bytes.len() as u64 > state.store.limits().write_bytes {
        return Err(limit());
    }
    let result = state
        .store
        .compare_exchange(
            state.scope,
            &value.key(),
            progress.version.clone(),
            Some(StateWrite {
                payload: bytes.into(),
                expires_at: Some(state.expires_at),
            }),
        )
        .await?;
    match result {
        CasResult::Applied(Some(version)) => {
            progress.version = Some(version);
            Ok(())
        }
        _ => Err(conflict(
            "journal CAS was not applied; reconcile retained evidence",
        )),
    }
}

pub(super) async fn load<S: StateStore, N>(
    expected: Journal,
    state: &GenerationStateAccess<'_, S>,
    limits: CodecLimits,
    progress: &mut FanoutProgress<N>,
) -> Result<(), TransformError> {
    if state.now >= state.expires_at {
        return Err(conflict("fanout invocation state expired"));
    }
    let entry = state
        .store
        .get(state.scope, &expected.key())
        .await?
        .ok_or_else(|| {
            TransformError::new(
                TransformErrorKind::MissingState,
                "generation.fanout",
                "reserved journal is missing",
            )
        })?;
    if entry.payload.len() as u64 > state.store.limits().read_bytes {
        return Err(limit());
    }
    if entry.expires_at != Some(state.expires_at) {
        return Err(conflict("journal expiry changed"));
    }
    let saved: Journal = crate::codec::decode_json(&entry.payload, limits).map_err(codec_error)?;
    if saved.schema != 1
        || saved.binding != expected.binding
        || saved.children.len() != expected.children.len()
    {
        return Err(conflict(
            "journal request, target, identity or child count differs",
        ));
    }
    if let Some(local) = &progress.journal {
        if local.binding != saved.binding || progress.version.as_ref() != Some(&entry.version) {
            return Err(conflict("journal changed outside retained progress"));
        }
        // Keep post-send evidence not yet durably acknowledged. Its version is
        // still the exact acknowledged journal version checked above.
    } else {
        for child in &saved.children {
            let p = GenerationProgress {
                send_started: child.started,
                raw_response: child.raw.as_ref().map(Raw::wire).transpose()?,
                saved_identities: unpack(&child.identities),
                ..Default::default()
            };
            if !child.started && child.raw.is_some() {
                return Err(conflict("result exists without a reserved send"));
            }
            progress.children.push(p);
        }
        progress.group.saved_identities = unpack(&saved.group_identities);
        progress.version = Some(entry.version);
        progress.journal = Some(saved);
    }
    Ok(())
}
