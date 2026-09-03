use std::collections::{BTreeMap, BTreeSet, VecDeque};

use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::generate_content::gemini_generate_content_to_openai_chat::tools::CODE_EXECUTION_NAME;

#[derive(Default)]
pub(super) struct State {
    next: BTreeMap<u32, u32>,
    ids: BTreeMap<(u32, String), u32>,
    pending: BTreeMap<u32, VecDeque<u32>>,
    open: BTreeSet<(u32, u32)>,
}

impl State {
    pub(super) fn function(
        &mut self,
        candidate: u32,
        call: gemini::FunctionCall,
    ) -> Result<openai::ChatToolCallDelta, TransformError> {
        let index = self.allocate(candidate);
        let id = call
            .id
            .unwrap_or_else(|| format!("gemini_call_{candidate}_{index}"));
        let args = call
            .args
            .ok_or_else(|| TransformError::shape("Gemini function call", "args is missing"))?;
        Ok(delta(index, id, call.name, serde_json::to_string(&args)?))
    }

    pub(super) fn code(
        &mut self,
        candidate: u32,
        mut code: gemini::ExecutableCode,
    ) -> Result<openai::ChatToolCallDelta, TransformError> {
        let index = self.allocate(candidate);
        let id = code
            .id
            .clone()
            .unwrap_or_else(|| format!("gemini_code_{candidate}_{index}"));
        code.id = Some(id.clone());
        self.ids.insert((candidate, id.clone()), index);
        self.pending.entry(candidate).or_default().push_back(index);
        self.open.insert((candidate, index));
        Ok(delta(
            index,
            id,
            CODE_EXECUTION_NAME.into(),
            serde_json::to_string(&code)?,
        ))
    }

    pub(super) fn result(
        &mut self,
        candidate: u32,
        result: gemini::CodeExecutionResult,
    ) -> Result<(), TransformError> {
        let index = result
            .id
            .as_ref()
            .and_then(|id| self.ids.get(&(candidate, id.clone())))
            .copied()
            .or_else(|| {
                self.pending
                    .get_mut(&candidate)
                    .and_then(VecDeque::pop_front)
            })
            .ok_or_else(|| {
                TransformError::shape(
                    "Gemini code execution result",
                    "no preceding executableCode",
                )
            })?;
        if !self.open.remove(&(candidate, index)) {
            return Err(TransformError::shape(
                "Gemini code execution result",
                "executableCode already has a result",
            ));
        }
        if let Some(pending) = self.pending.get_mut(&candidate)
            && let Some(position) = pending.iter().position(|value| *value == index)
        {
            pending.remove(position);
        }
        Ok(())
    }

    pub(super) fn complete(&self) -> bool {
        self.open.is_empty()
    }

    fn allocate(&mut self, candidate: u32) -> u32 {
        let next = self.next.entry(candidate).or_insert(0);
        let value = *next;
        *next = next.saturating_add(1);
        value
    }
}

fn delta(index: u32, id: String, name: String, arguments: String) -> openai::ChatToolCallDelta {
    crate::wire!(openai::ChatToolCallDelta {
        index,
        id: Some(id),
        type_: Some(openai::ChatToolCallType::Function),
        function: Some(openai::FunctionCallDelta {
            arguments: Some(arguments),
            name: Some(name),
            rest: Default::default(),
        }),
        custom: None,
        rest: Default::default(),
    })
}
