use super::parts::Part;
use super::*;
use std::collections::BTreeMap;

pub(super) struct Item {
    pub value: r::ResponseOutputItem,
    pub done: bool,
    pub parts: BTreeMap<(bool, i64), Part>,
    pub argument_done: bool,
    pub argument_streamed: bool,
    progress_phase: Option<u8>,
}

pub(super) fn item_id(item: &r::ResponseOutputItem) -> Option<&str> {
    match item {
        r::ResponseOutputItem::Message(v) => Some(&v.id),
        r::ResponseOutputItem::FileSearchCall(v) => Some(&v.id),
        r::ResponseOutputItem::FunctionCall(v) => v.id.as_deref(),
        r::ResponseOutputItem::FunctionCallOutput(v) => Some(&v.id),
        r::ResponseOutputItem::WebSearchCall(v) => Some(&v.id),
        r::ResponseOutputItem::ComputerCall(v) => Some(&v.id),
        r::ResponseOutputItem::ComputerCallOutput(v) => Some(&v.id),
        r::ResponseOutputItem::Reasoning(v) => Some(&v.id),
        r::ResponseOutputItem::Program(v) => Some(&v.id),
        r::ResponseOutputItem::ProgramOutput(v) => Some(&v.id),
        r::ResponseOutputItem::ToolSearchCall(v) => Some(&v.id),
        r::ResponseOutputItem::ToolSearchOutput(v) => Some(&v.id),
        r::ResponseOutputItem::AdditionalTools(v) => Some(&v.id),
        r::ResponseOutputItem::Compaction(v) => Some(&v.id),
        r::ResponseOutputItem::ImageGenerationCall(v) => Some(&v.id),
        r::ResponseOutputItem::CodeInterpreterCall(v) => Some(&v.id),
        r::ResponseOutputItem::LocalShellCall(v) => Some(&v.id),
        r::ResponseOutputItem::LocalShellCallOutput(v) => Some(&v.id),
        r::ResponseOutputItem::ShellCall(v) => Some(&v.id),
        r::ResponseOutputItem::ShellCallOutput(v) => Some(&v.id),
        r::ResponseOutputItem::ApplyPatchCall(v) => Some(&v.id),
        r::ResponseOutputItem::ApplyPatchCallOutput(v) => Some(&v.id),
        r::ResponseOutputItem::McpCall(v) => Some(&v.id),
        r::ResponseOutputItem::McpListTools(v) => Some(&v.id),
        r::ResponseOutputItem::McpApprovalRequest(v) => Some(&v.id),
        r::ResponseOutputItem::McpApprovalResponse(v) => Some(&v.id),
        r::ResponseOutputItem::CustomToolCall(v) => v.id.as_deref(),
        r::ResponseOutputItem::CustomToolCallOutput(v) => Some(&v.id),
    }
}

impl Item {
    pub fn new(mut value: r::ResponseOutputItem) -> Self {
        let mut parts = BTreeMap::new();
        match &mut value {
            r::ResponseOutputItem::Message(message) => {
                for (index, part) in std::mem::take(&mut message.content).into_iter().enumerate() {
                    let value = match part {
                        i::OutputContent::Text(value) => s::OutputContentPart::Text(value),
                        i::OutputContent::Refusal(value) => s::OutputContentPart::Refusal(value),
                    };
                    let mut part = Part::new(value, false);
                    part.seeded = true;
                    parts.insert((false, index as i64), part);
                }
            }
            r::ResponseOutputItem::Reasoning(reasoning) => {
                for (index, part) in std::mem::take(&mut reasoning.summary)
                    .into_iter()
                    .enumerate()
                {
                    let mut part = Part::new(
                        s::OutputContentPart::Reasoning(s::ReasoningText {
                            text: part.text,
                            type_: s::ReasoningTextType::ReasoningText,
                            rest: Default::default(),
                        }),
                        true,
                    );
                    part.seeded = true;
                    parts.insert((true, index as i64), part);
                }
                if let Some(content) = &mut reasoning.content {
                    for (index, part) in std::mem::take(content).into_iter().enumerate() {
                        let mut part = Part::new(
                            s::OutputContentPart::Reasoning(s::ReasoningText {
                                text: part.text,
                                type_: s::ReasoningTextType::ReasoningText,
                                rest: Default::default(),
                            }),
                            false,
                        );
                        part.seeded = true;
                        parts.insert((false, index as i64), part);
                    }
                }
            }
            _ => {}
        }
        Self {
            value,
            done: false,
            parts,
            argument_done: false,
            argument_streamed: false,
            progress_phase: None,
        }
    }
    pub fn arguments(&self) -> Result<&str, TransformError> {
        match &self.value {
            r::ResponseOutputItem::FunctionCall(v) => Ok(&v.arguments),
            r::ResponseOutputItem::CustomToolCall(v) => Ok(&v.input),
            r::ResponseOutputItem::McpCall(v) => Ok(&v.arguments),
            r::ResponseOutputItem::CodeInterpreterCall(v) => Ok(v.code.as_deref().unwrap_or("")),
            _ => Err(invalid("argument event on wrong item type")),
        }
    }
    pub fn arguments_mut(&mut self) -> Result<&mut String, TransformError> {
        match &mut self.value {
            r::ResponseOutputItem::FunctionCall(v) => Ok(&mut v.arguments),
            r::ResponseOutputItem::CustomToolCall(v) => Ok(&mut v.input),
            r::ResponseOutputItem::McpCall(v) => Ok(&mut v.arguments),
            r::ResponseOutputItem::CodeInterpreterCall(v) => Ok(v.code.get_or_insert_default()),
            _ => Err(invalid("argument event on wrong item type")),
        }
    }
    pub fn finish(&mut self, final_item: r::ResponseOutputItem) -> Result<(), TransformError> {
        if self.done
            || std::mem::discriminant(&self.value) != std::mem::discriminant(&final_item)
            || item_id(&self.value) != item_id(&final_item)
        {
            return Err(invalid("item identity/type changed or duplicate done"));
        }
        super::status::item(&final_item, false)?;
        self.check_progress(&final_item)?;
        if !same_links(&self.value, &final_item) {
            return Err(invalid("output item call/reference identity changed"));
        }
        match (&self.value, &final_item) {
            (r::ResponseOutputItem::Message(a), r::ResponseOutputItem::Message(b)) => {
                if a.role != b.role
                    || b.content.len()
                        != a.content.len()
                            + self.parts.keys().filter(|(summary, _)| !summary).count()
                {
                    return Err(invalid("message parts missing or duplicated"));
                }
                if !b.content.starts_with(&a.content) {
                    return Err(invalid("initial message contents changed"));
                }
            }
            (r::ResponseOutputItem::Reasoning(a), r::ResponseOutputItem::Reasoning(b)) => {
                if b.summary.len()
                    != a.summary.len() + self.parts.keys().filter(|(summary, _)| *summary).count()
                    || !b.summary.starts_with(&a.summary)
                    || b.content.as_deref().unwrap_or_default().len()
                        != a.content.as_deref().unwrap_or_default().len()
                            + self.parts.keys().filter(|(summary, _)| !summary).count()
                    || !b
                        .content
                        .as_deref()
                        .unwrap_or_default()
                        .starts_with(a.content.as_deref().unwrap_or_default())
                {
                    return Err(invalid("reasoning parts missing or changed"));
                }
            }
            (r::ResponseOutputItem::FunctionCall(a), r::ResponseOutputItem::FunctionCall(b))
                if a.call_id != b.call_id
                    || a.name != b.name
                    || a.namespace != b.namespace
                    || a.caller != b.caller
                    || a.arguments != b.arguments =>
            {
                return Err(invalid("function identity or arguments changed"));
            }
            (
                r::ResponseOutputItem::CustomToolCall(a),
                r::ResponseOutputItem::CustomToolCall(b),
            ) if a.call_id != b.call_id
                || a.name != b.name
                || a.namespace != b.namespace
                || a.caller != b.caller
                || a.input != b.input =>
            {
                return Err(invalid("custom tool identity or input changed"));
            }
            (r::ResponseOutputItem::McpCall(a), r::ResponseOutputItem::McpCall(b))
                if a.name != b.name
                    || a.server_label != b.server_label
                    || a.arguments != b.arguments =>
            {
                return Err(invalid("MCP identity or arguments changed"));
            }
            (
                r::ResponseOutputItem::CodeInterpreterCall(a),
                r::ResponseOutputItem::CodeInterpreterCall(b),
            ) if self.argument_streamed && a.code != b.code => {
                return Err(invalid("code deltas contradict final code"));
            }
            _ => {}
        }
        if self.argument_streamed && !self.argument_done {
            return Err(invalid("argument stream missing done"));
        }
        for ((summary, index), part) in &self.parts {
            part.check_final(&final_item, *summary, *index)?;
        }
        self.value = final_item;
        self.done = true;
        Ok(())
    }
    pub fn base_parts(&self, summary: bool) -> Result<usize, TransformError> {
        match (&self.value, summary) {
            (r::ResponseOutputItem::Message(v), false) => Ok(v.content.len()),
            (r::ResponseOutputItem::Reasoning(v), true) => Ok(v.summary.len()),
            (r::ResponseOutputItem::Reasoning(v), false) => {
                Ok(v.content.as_ref().map_or(0, Vec::len))
            }
            _ => Err(invalid("content event on wrong item type")),
        }
    }
}

impl Item {
    pub fn progress(&mut self, kind: u8, phase: u8) -> Result<(), TransformError> {
        let actual = match self.value {
            r::ResponseOutputItem::ImageGenerationCall(_) => 0,
            r::ResponseOutputItem::CodeInterpreterCall(_) => 1,
            r::ResponseOutputItem::FileSearchCall(_) => 2,
            r::ResponseOutputItem::WebSearchCall(_) => 3,
            r::ResponseOutputItem::McpCall(_) => 4,
            r::ResponseOutputItem::McpListTools(_) => 5,
            _ => 255,
        };
        if kind != actual
            || self
                .progress_phase
                .is_some_and(|old| old >= 2 || phase < old)
        {
            return Err(invalid("tool progress type/phase mismatch"));
        }
        self.progress_phase = Some(phase);
        Ok(())
    }
    fn check_progress(&self, item: &r::ResponseOutputItem) -> Result<(), TransformError> {
        let Some(phase) = self.progress_phase.filter(|v| *v >= 2) else {
            return Ok(());
        };
        let success = phase == 2;
        let valid = match item {
            r::ResponseOutputItem::ImageGenerationCall(v) => {
                success && v.status == i::ImageGenerationStatus::Completed
            }
            r::ResponseOutputItem::CodeInterpreterCall(v) => {
                success && v.status == i::CodeInterpreterStatus::Completed
            }
            r::ResponseOutputItem::FileSearchCall(v) => {
                success && v.status == i::FileSearchStatus::Completed
            }
            r::ResponseOutputItem::WebSearchCall(v) => {
                success && v.status == i::WebSearchStatus::Completed
            }
            r::ResponseOutputItem::McpCall(v) => {
                v.status
                    == Some(if success {
                        i::McpCallStatus::Completed
                    } else {
                        i::McpCallStatus::Failed
                    })
            }
            r::ResponseOutputItem::McpListTools(v) => {
                success != v.error.as_ref().and_then(|v| v.as_ref()).is_some()
            }
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(invalid("final tool item contradicts progress terminal"))
        }
    }
}

fn same_links(a: &r::ResponseOutputItem, b: &r::ResponseOutputItem) -> bool {
    match (a, b) {
        (
            r::ResponseOutputItem::FunctionCallOutput(a),
            r::ResponseOutputItem::FunctionCallOutput(b),
        ) => {
            a.call_id == b.call_id
                && a.name == b.name
                && a.namespace == b.namespace
                && a.caller == b.caller
        }
        (r::ResponseOutputItem::ComputerCall(a), r::ResponseOutputItem::ComputerCall(b)) => {
            a.call_id == b.call_id && a.action == b.action && a.actions == b.actions
        }
        (
            r::ResponseOutputItem::ComputerCallOutput(a),
            r::ResponseOutputItem::ComputerCallOutput(b),
        ) => a.call_id == b.call_id,
        (r::ResponseOutputItem::Program(a), r::ResponseOutputItem::Program(b)) => {
            a.call_id == b.call_id && a.code == b.code && a.fingerprint == b.fingerprint
        }
        (r::ResponseOutputItem::ProgramOutput(a), r::ResponseOutputItem::ProgramOutput(b)) => {
            a.call_id == b.call_id
        }
        (r::ResponseOutputItem::ToolSearchCall(a), r::ResponseOutputItem::ToolSearchCall(b)) => {
            a.call_id == b.call_id
        }
        (
            r::ResponseOutputItem::ToolSearchOutput(a),
            r::ResponseOutputItem::ToolSearchOutput(b),
        ) => a.call_id == b.call_id,
        (r::ResponseOutputItem::LocalShellCall(a), r::ResponseOutputItem::LocalShellCall(b)) => {
            a.call_id == b.call_id && a.action == b.action
        }
        (r::ResponseOutputItem::ShellCall(a), r::ResponseOutputItem::ShellCall(b)) => {
            a.call_id == b.call_id
                && a.action == b.action
                && a.environment == b.environment
                && a.caller == b.caller
        }
        (r::ResponseOutputItem::ShellCallOutput(a), r::ResponseOutputItem::ShellCallOutput(b)) => {
            a.call_id == b.call_id
        }
        (r::ResponseOutputItem::ApplyPatchCall(a), r::ResponseOutputItem::ApplyPatchCall(b)) => {
            a.call_id == b.call_id && a.operation == b.operation && a.caller == b.caller
        }
        (
            r::ResponseOutputItem::ApplyPatchCallOutput(a),
            r::ResponseOutputItem::ApplyPatchCallOutput(b),
        ) => a.call_id == b.call_id,
        (
            r::ResponseOutputItem::CustomToolCallOutput(a),
            r::ResponseOutputItem::CustomToolCallOutput(b),
        ) => a.call_id == b.call_id,
        (
            r::ResponseOutputItem::McpApprovalResponse(a),
            r::ResponseOutputItem::McpApprovalResponse(b),
        ) => a.approval_request_id == b.approval_request_id,
        (
            r::ResponseOutputItem::McpApprovalRequest(a),
            r::ResponseOutputItem::McpApprovalRequest(b),
        ) => a.name == b.name && a.server_label == b.server_label && a.arguments == b.arguments,
        (r::ResponseOutputItem::McpListTools(a), r::ResponseOutputItem::McpListTools(b)) => {
            a.server_label == b.server_label
        }
        (
            r::ResponseOutputItem::CodeInterpreterCall(a),
            r::ResponseOutputItem::CodeInterpreterCall(b),
        ) => a.container_id == b.container_id,
        _ => true,
    }
}
