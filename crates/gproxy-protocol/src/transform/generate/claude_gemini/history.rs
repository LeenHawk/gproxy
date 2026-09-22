use crate::{
    transform::{
        Report, TransformError,
        identity::{IdentityFlow, IdentityRole, SourceIdentity, TargetIdPolicy},
    },
    wire::{claude::content as c, gemini as g},
};
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct Calls {
    index: u64,
    // native ID -> (target ID, name, pending)
    calls: BTreeMap<String, (String, String, bool)>,
    seeded: std::collections::BTreeSet<String>,
}

impl Calls {
    pub(crate) fn seed(&mut self, id: String, name: String) -> Result<(), TransformError> {
        if id.is_empty() || name.is_empty() {
            return Err(TransformError::shape("tool_names", "empty ID/name"));
        }
        if self.calls.get(&id).is_some_and(|(_, old, _)| old != &name) {
            return Err(TransformError::shape(
                "tool_names",
                "conflicting name for ID",
            ));
        }
        self.seeded.insert(id.clone());
        self.calls.insert(id.clone(), (id, name, true));
        Ok(())
    }
    pub(crate) fn call(
        &mut self,
        id: Option<String>,
        name: &str,
        dialect: crate::Dialect,
        flow: &mut IdentityFlow,
        policy: &TargetIdPolicy,
    ) -> Result<String, TransformError> {
        if name.is_empty() || id.as_ref().is_some_and(String::is_empty) {
            return Err(TransformError::shape("tool_call", "empty ID/name"));
        }
        if let Some(key) = &id
            && self.seeded.remove(key)
        {
            let (_, old, _) = self.calls.remove(key).expect("seed binding");
            if old != name {
                return Err(TransformError::shape(
                    "tool_names",
                    "history and supplement conflict",
                ));
            }
        }
        if id.as_ref().is_some_and(|id| self.calls.contains_key(id)) {
            return Err(TransformError::shape("tool_call.id", "duplicate ID"));
        }
        let key = id.clone();
        let value = flow
            .resolve_or_allocate(
                IdentityRole::ToolCall,
                SourceIdentity::new(dialect, id, self.index),
                policy,
            )
            .map_err(|e| TransformError::shape("identity", e.to_string()))?
            .emitted_id;
        self.index += 1;
        self.calls.insert(
            key.unwrap_or_else(|| value.clone()),
            (value.clone(), name.into(), true),
        );
        Ok(value)
    }
    pub(crate) fn result(
        &mut self,
        id: Option<&str>,
        name: Option<&str>,
    ) -> Result<(String, String), TransformError> {
        let key = if let Some(id) = id {
            id.to_owned()
        } else {
            let keys: Vec<_> = self
                .calls
                .iter()
                .filter(|(_, (_, n, p))| *p && Some(n.as_str()) == name)
                .map(|(k, _)| k.clone())
                .collect();
            if keys.len() != 1 {
                return Err(TransformError::missing_metadata(
                    "unique tool result binding",
                ));
            }
            keys[0].clone()
        };
        let (target, n, pending) = self
            .calls
            .get_mut(&key)
            .ok_or_else(|| TransformError::missing_metadata("tool result name/ID history"))?;
        if !*pending || name.is_some_and(|v| v != n) {
            return Err(TransformError::shape(
                "tool_result",
                "duplicate result or mismatched name",
            ));
        }
        *pending = false;
        Ok((target.clone(), n.clone()))
    }
}

pub(crate) fn to_gemini(
    blocks: Vec<c::ContentBlock>,
    media: &super::media::MediaFacts,
    calls: &mut Calls,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    report: &mut Report,
) -> Result<Vec<g::Part>, TransformError> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            c::ContentBlock::Text(v) => {
                if v.citations.is_some() || v.cache_control.is_some() {
                    report.omitted(
                        "text.citations/cache_control",
                        "Gemini history has no matching fields",
                    );
                }
                out.push(g::Part::builder().text(v.text).build());
            }
            c::ContentBlock::Image(v) => {
                if let Some(part) =
                    crate::transform::optional(super::media::image(v, media, report))?
                {
                    out.push(part);
                }
            }
            c::ContentBlock::Document(v) => {
                if let Some(parts) =
                    crate::transform::optional(super::media::document(v, media, report))?
                {
                    out.extend(parts);
                }
            }
            c::ContentBlock::ToolUse(v) if v.toolset_name.is_some() => {
                report.omitted(
                    "toolset_name",
                    "native toolset member has no target definition",
                );
                continue;
            }
            c::ContentBlock::ToolUse(v) => {
                if v.caller.is_some() {
                    report.omitted("tool_use.caller", "caller has no target representation");
                }
                let args = v.input.as_object().cloned().ok_or_else(|| {
                    TransformError::shape("tool_use.input", "object arguments required")
                })?;
                let id = calls.call(Some(v.id), &v.name, crate::Dialect::Claude, flow, policy)?;
                out.push(
                    g::Part::builder()
                        .function_call(g::FunctionCall::builder(v.name).id(id).args(args).build())
                        .build(),
                );
                if v.cache_control.is_some() {
                    report.omitted("tool_use.cache_control", "Gemini has no block cache field");
                }
            }
            c::ContentBlock::ToolResult(v) if v.toolset_name.is_some() => {
                report.omitted(
                    "toolset_name",
                    "native toolset member has no target definition",
                );
                continue;
            }
            c::ContentBlock::ToolResult(v) => {
                let (id, name) = calls.result(Some(&v.tool_use_id), None)?;
                if !policy.accepts_source(&id) {
                    return Err(TransformError::missing_metadata(
                        "seeded tool result identity must satisfy selected target policy",
                    ));
                }
                out.push(
                    g::Part::builder()
                        .function_response(super::results::to_gemini(v, id, name, media, report)?)
                        .build(),
                );
            }
            c::ContentBlock::Thinking(v) => {
                report.omitted("thinking.signature","native Claude signature retained by host; Gemini receives only declared thinking text");
                out.push(g::Part::builder().thought(true).text(v.thinking).build());
            }
            c::ContentBlock::RedactedThinking(_) => report.omitted(
                "redacted_thinking",
                "native opaque Claude reasoning has no Gemini representation",
            ),
            c::ContentBlock::ServerToolUse(_)
            | c::ContentBlock::SearchResult(_)
            | c::ContentBlock::WebSearchToolResult(_)
            | c::ContentBlock::WebFetchToolResult(_)
            | c::ContentBlock::AdvisorToolResult(_)
            | c::ContentBlock::CodeExecutionToolResult(_)
            | c::ContentBlock::BashCodeExecutionToolResult(_)
            | c::ContentBlock::TextEditorCodeExecutionToolResult(_)
            | c::ContentBlock::ToolSearchToolResult(_)
            | c::ContentBlock::McpToolUse(_)
            | c::ContentBlock::McpToolResult(_)
            | c::ContentBlock::ContainerUpload(_)
            | c::ContentBlock::McpToolListing(_)
            | c::ContentBlock::Compaction(_)
            | c::ContentBlock::MidConversationSystem(_)
            | c::ContentBlock::ToolAddition(_)
            | c::ContentBlock::ToolRemoval(_)
            | c::ContentBlock::Fallback(_) => {
                continue;
            }
        }
    }
    Ok(out)
}

pub(crate) fn to_claude(
    parts: Vec<g::Part>,
    calls: &mut Calls,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    report: &mut Report,
) -> Result<Vec<c::ContentBlock>, TransformError> {
    let mut out = Vec::new();
    for p in parts {
        if p.part_metadata.is_some() {
            report.omitted("part_metadata", "no Claude part metadata field");
        }
        if p.thought_signature.is_some() {
            report.omitted(
                "thought_signature",
                "native signature requires scoped host state; cannot be used as Claude signature",
            );
        }
        if p.thought == Some(true) {
            if p.text.is_none() {
                continue;
            }
            report.omitted(
                "thought",
                "Claude signed thinking requires native signature",
            );
            continue;
        }
        if let Some(text) = p.text {
            out.push(c::ContentBlock::Text(
                c::TextBlock::builder(c::TextBlockType::Tag, text).build(),
            ));
        }
        if let Some(blob) = p.inline_data
            && let Some(block) = crate::transform::optional(super::media::inline(blob))?
        {
            out.push(block);
        }
        if let Some(file) = p.file_data
            && let Some(block) = crate::transform::optional(super::media::file(file))?
        {
            out.push(block);
        }
        if let Some(call) = p.function_call {
            let id = calls.call(call.id, &call.name, crate::Dialect::Gemini, flow, policy)?;
            out.push(c::ContentBlock::ToolUse(
                c::ToolUseBlock::builder(
                    c::ToolUseBlockType::Tag,
                    id,
                    serde_json::Value::Object(call.args.unwrap_or_default()),
                    call.name,
                )
                .build(),
            ));
        }
        if let Some(result) = p.function_response {
            let (id, _) = calls.result(result.id.as_deref(), Some(&result.name))?;
            out.push(c::ContentBlock::ToolResult(super::results::to_claude(
                result, id,
            )?));
        }
    }
    Ok(out)
}
