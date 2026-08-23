mod functions;
mod media;
mod native;
mod request;
mod request_meta;
mod response;

use std::collections::{BTreeMap, VecDeque};

use crate::TransformError;

pub(crate) use request::request_messages;
pub(crate) use request_meta::system;
pub(crate) use response::{response_blocks, response_part};

#[derive(Default)]
pub(crate) struct Correlation {
    next: u64,
    functions: BTreeMap<String, VecDeque<String>>,
    code_calls: VecDeque<String>,
}

impl Correlation {
    pub(super) fn function_call(&mut self, id: Option<String>, name: &str) -> String {
        let id = id.unwrap_or_else(|| self.stable_id("call"));
        self.functions
            .entry(name.to_owned())
            .or_default()
            .push_back(id.clone());
        id
    }

    pub(super) fn function_result(
        &mut self,
        id: Option<String>,
        name: &str,
    ) -> Result<String, TransformError> {
        if let Some(id) = id {
            let ids = self.functions.get_mut(name).ok_or_else(|| {
                TransformError::shape("Gemini function response", "matching call is missing")
            })?;
            let position = ids
                .iter()
                .position(|candidate| candidate == &id)
                .ok_or_else(|| {
                    TransformError::shape("Gemini function response", "call id is unknown")
                })?;
            ids.remove(position);
            return Ok(id);
        }
        self.functions
            .get_mut(name)
            .and_then(VecDeque::pop_front)
            .ok_or_else(|| {
                TransformError::shape("Gemini function response", "matching call id is missing")
            })
    }

    pub(super) fn code_call(&mut self, id: Option<String>) -> String {
        let id = id.unwrap_or_else(|| self.stable_id("code"));
        self.code_calls.push_back(id.clone());
        id
    }

    pub(super) fn code_result(&mut self, id: Option<String>) -> Result<String, TransformError> {
        if let Some(id) = id {
            let position = self
                .code_calls
                .iter()
                .position(|candidate| candidate == &id)
                .ok_or_else(|| {
                    TransformError::shape("Gemini code result", "executable id is unknown")
                })?;
            self.code_calls.remove(position);
            return Ok(id);
        }
        self.code_calls.pop_front().ok_or_else(|| {
            TransformError::shape("Gemini code result", "matching executable id is missing")
        })
    }

    fn stable_id(&mut self, kind: &str) -> String {
        let id = format!("gemini_{kind}_{}", self.next);
        self.next = self.next.saturating_add(1);
        id
    }
}

pub(super) fn merge(
    mut left: serde_json::Map<String, serde_json::Value>,
    right: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    left.extend(right);
    left
}

pub(super) fn caller(signature: Option<String>) -> Option<gproxy_protocol::claude::Caller> {
    let signature = signature?;
    Some(gproxy_protocol::claude::Caller::Direct(
        gproxy_protocol::claude::DirectCaller {
            type_: gproxy_protocol::claude::DirectCallerType::Direct,
            rest: [("thought_signature".into(), signature.into())]
                .into_iter()
                .collect(),
        },
    ))
}
