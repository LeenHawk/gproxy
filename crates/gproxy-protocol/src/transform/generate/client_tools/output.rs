use super::*;
use crate::transform::identity::OutputItemKind;

impl Bindings {
    pub(crate) fn kind(&self, name: &str) -> OutputItemKind {
        match self.entries.get(name) {
            Some(Kind::Custom { .. }) => OutputItemKind::CustomToolCall,
            Some(Kind::Shell) => OutputItemKind::ShellCall,
            Some(Kind::Patch) => OutputItemKind::ApplyPatchCall,
            Some(Kind::Search) => OutputItemKind::ToolSearchCall,
            _ => OutputItemKind::FunctionCall,
        }
    }
    pub(crate) fn restore(
        &self,
        mut call: r::FunctionCall,
    ) -> Result<r::ResponseOutputItem, TransformError> {
        if self.hidden.contains(&call.name) {
            return Err(TransformError::invalid_result(
                "client_tools",
                "returned deferred tool has not been discovered",
            ));
        }
        let Some(kind) = self.entries.get(&call.name) else {
            return Ok(r::ResponseOutputItem::FunctionCall(call));
        };
        if !self
            .tools
            .iter()
            .any(|tool| matches!(tool, r::Tool::Function(tool) if tool.name == call.name))
        {
            return Err(TransformError::invalid_result(
                "client_tools",
                "returned tool was not activated in the request catalog",
            ));
        }
        if let Kind::Namespace { namespace, name } = kind {
            call.name = name.clone();
            call.namespace = Some(namespace.clone());
            return Ok(r::ResponseOutputItem::FunctionCall(call));
        }
        let value: Value = serde_json::from_str(&call.arguments)
            .map_err(|e| TransformError::invalid_result("client_tools.arguments", e.to_string()))?;
        let id = call.id;
        let wire = match kind {
            Kind::Custom { name } => {
                let input = value.get("input").and_then(Value::as_str).ok_or_else(|| {
                    TransformError::invalid_result(
                        "custom_tool.input",
                        "expected the raw tool input string",
                    )
                })?;
                json!({"type":"custom_tool_call","id":id,"call_id":call.call_id,"name":name,"input":input})
            }
            Kind::Shell => {
                let action: r::ShellAction = serde_json::from_value(value)
                    .map_err(|e| TransformError::invalid_result("shell.action", e.to_string()))?;
                if action.commands.is_empty()
                    || action.timeout_ms.flatten().is_some_and(|n| n < 0)
                    || action.max_output_length.flatten().is_some_and(|n| n < 0)
                {
                    return Err(TransformError::invalid_result(
                        "shell.action",
                        "empty commands or negative limits",
                    ));
                }
                json!({"type":"shell_call","id":id,"call_id":call.call_id,"action":action.into_declared(),"environment":{"type":"local"},"status":call.status})
            }
            Kind::Patch => {
                if call.status == Some(r::ItemStatus::Incomplete) {
                    return Err(TransformError::invalid_result(
                        "apply_patch",
                        "incomplete arguments cannot become a completed patch operation",
                    ));
                }
                let operation: r::ApplyPatchOperation =
                    serde_json::from_value(value).map_err(|e| {
                        TransformError::invalid_result("apply_patch.operation", e.to_string())
                    })?;
                json!({"type":"apply_patch_call","id":id,"call_id":call.call_id,"operation":operation.into_declared(),"status":if call.status == Some(r::ItemStatus::InProgress) { "in_progress" } else { "completed" }})
            }
            Kind::Search => {
                json!({"type":"tool_search_call","id":id,"call_id":call.call_id,"arguments":value,"execution":"client","status":call.status})
            }
            // Namespace kinds are filtered out before a call reaches output mapping.
            Kind::Namespace { .. } => unreachable!(),
        };
        serde_json::from_value(wire)
            .map_err(|e| TransformError::invalid_result("client_tools.output", e.to_string()))
    }
}
