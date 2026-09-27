use super::*;

#[derive(Default)]
pub(super) struct ChatTool {
    pub name: String,
    pub arguments: String,
    pub id: Option<String>,
    pub ordinal: u64,
}

impl ChatTool {
    pub fn append(
        &mut self,
        id: Option<String>,
        function: Option<s::DeltaFunctionCall>,
    ) -> Result<(), TransformError> {
        if let Some(id) = id {
            if id.is_empty() || self.id.as_ref().is_some_and(|v| v != &id) {
                return Err(invalid("tool identity changed"));
            }
            self.id = Some(id);
        }
        if let Some(function) = function {
            if let Some(name) = function.name.flatten() {
                self.name.push_str(&name);
            }
            if let Some(args) = function.arguments.flatten() {
                self.arguments.push_str(&args);
            }
        }
        Ok(())
    }
    pub fn part(
        self,
        flow: &mut IdentityFlow,
        legacy: bool,
        policy: &TargetIdPolicy,
    ) -> Result<g::Part, TransformError> {
        if self.name.is_empty() {
            return Err(TransformError::missing_metadata("tool.name"));
        }
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&self.arguments).map_err(|e| {
                TransformError::invalid_result(
                    "tool.arguments",
                    format!("Gemini requires a complete JSON object: {e}"),
                )
            })?;
        let mut call = g::FunctionCall::builder(self.name).args(args).build();
        // Both modern and legacy calls need a client ID. A legacy call has
        // none upstream, and its alias says it goes back in the legacy form.
        call.id = Some(if legacy {
            flow.resolve_legacy_chat_call(
                SourceIdentity::new(crate::Dialect::OpenAiChat, None, self.ordinal),
                policy,
            )
            .map(|v| v.emitted_id)
            .map_err(|e| TransformError::invalid_result("stream.identity", e.to_string()))?
        } else {
            super::common::id(
                flow,
                policy,
                IdentityRole::ToolCall,
                crate::Dialect::OpenAiChat,
                self.id,
                self.ordinal,
            )?
        });
        Ok(g::Part::builder().function_call(call).build())
    }
}

#[derive(Default)]
pub(super) struct ChatChoice {
    pub tools: BTreeMap<i64, ChatTool>,
    pub legacy: Option<ChatTool>,
    pub refusal: Option<String>,
}
