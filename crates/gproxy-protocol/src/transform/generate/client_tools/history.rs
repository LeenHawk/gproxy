use super::*;

fn call(
    name: &str,
    call_id: String,
    id: Option<String>,
    value: impl serde::Serialize,
) -> Result<r::InputItem, TransformError> {
    if call_id.is_empty() {
        return Err(unsupported("client tool call_id required"));
    }
    let arguments = serde_json::to_string(&value)
        .map_err(|e| TransformError::shape("client_tools.arguments", e.to_string()))?;
    let mut out = r::FunctionCall::builder(
        r::FunctionCallType::FunctionCall,
        arguments,
        call_id,
        name.to_owned(),
    )
    .build();
    out.id = id;
    Ok(r::InputItem::FunctionCall(out))
}

fn result(call_id: String, value: impl serde::Serialize) -> Result<r::InputItem, TransformError> {
    if call_id.is_empty() {
        return Err(unsupported("client tool output call_id required"));
    }
    let text = serde_json::to_string(&value)
        .map_err(|e| TransformError::shape("client_tools.output", e.to_string()))?;
    Ok(r::InputItem::FunctionCallOutput(
        r::FunctionCallOutput::builder(
            r::FunctionCallOutputType::FunctionCallOutput,
            call_id,
            r::FunctionOutput::Text(text),
        )
        .build(),
    ))
}

fn caller(value: &Option<Option<r::Caller>>) -> Result<(), TransformError> {
    if matches!(value, Some(Some(r::Caller::Program(_)))) {
        return Err(unsupported("server-owned calls cannot become client calls"));
    }
    Ok(())
}

impl Bindings {
    pub(super) fn namespace_name(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<String, TransformError> {
        if let Some(Kind::Namespace {
            namespace: expected,
            ..
        }) = self.entries.get(name)
            && expected == namespace
        {
            return Ok(name.into());
        }
        let alias = qualified(namespace, name);
        if !matches!(self.entries.get(&alias), Some(Kind::Namespace { .. })) {
            return Err(unsupported(
                "namespaced history requires its declared tool binding",
            ));
        }
        Ok(alias)
    }
    pub(super) fn history(
        &self,
        input: &mut Option<r::Input>,
        report: &mut Report,
    ) -> Result<(), TransformError> {
        let Some(r::Input::Items(items)) = input else {
            return Ok(());
        };
        let mut lowered = Vec::new();
        for item in std::mem::take(items) {
            let mapped = (|| -> Result<Option<r::InputItem>, TransformError> {
                Ok(Some(match item {
                    r::InputItem::CustomToolCall(item)
                        if self.entries.contains_key(&custom_alias(&item.name)) =>
                    {
                        caller(&item.caller)?;
                        call(
                            &custom_alias(&item.name),
                            item.call_id,
                            item.id,
                            json!({"input":item.input}),
                        )?
                    }
                    r::InputItem::CustomToolCallOutput(item)
                        if self
                            .entries
                            .values()
                            .any(|kind| matches!(kind, Kind::Custom { .. })) =>
                    {
                        caller(&item.caller)?;
                        r::InputItem::FunctionCallOutput(
                            r::FunctionCallOutput::builder(
                                r::FunctionCallOutputType::FunctionCallOutput,
                                item.call_id,
                                serde_json::from_value(serde_json::to_value(item.output)?)?,
                            )
                            .build(),
                        )
                    }
                    r::InputItem::FunctionCall(mut item) => {
                        if let Some(namespace) = item.namespace.take() {
                            item.name = self.namespace_name(&namespace, &item.name)?;
                        }
                        r::InputItem::FunctionCall(item)
                    }
                    r::InputItem::FunctionCallOutput(mut item) => {
                        if let Some(Some(namespace)) = item.namespace.take()
                            && let Some(Some(name)) = &mut item.name
                        {
                            *name = self.namespace_name(&namespace, name)?;
                        }
                        r::InputItem::FunctionCallOutput(item)
                    }
                    r::InputItem::ShellCall(item) => {
                        caller(&item.caller)?;
                        if item
                            .environment
                            .flatten()
                            .is_some_and(|v| !matches!(v, r::ShellCallEnvironment::Local(_)))
                        {
                            return Err(unsupported(
                                "hosted shell history cannot be replayed as local shell",
                            ));
                        }
                        call(
                            SHELL,
                            item.call_id,
                            item.id.flatten(),
                            item.action.into_declared(),
                        )?
                    }
                    r::InputItem::ShellCallOutput(item) => {
                        caller(&item.caller)?;
                        result(
                            item.call_id,
                            json!({"output":item.output.into_declared(),"max_output_length":item.max_output_length}),
                        )?
                    }
                    r::InputItem::ApplyPatchCall(item) => {
                        caller(&item.caller)?;
                        call(
                            PATCH,
                            item.call_id,
                            item.id.flatten(),
                            item.operation.into_declared(),
                        )?
                    }
                    r::InputItem::ApplyPatchCallOutput(item) => {
                        caller(&item.caller)?;
                        result(
                            item.call_id,
                            json!({"status":item.status,"output":item.output}),
                        )?
                    }
                    r::InputItem::ToolSearchCall(item) => {
                        if item.execution != Some(r::ToolExecution::Client) {
                            return Err(unsupported(
                                "server ToolSearch history needs its native executor",
                            ));
                        }
                        call(
                            SEARCH,
                            item.call_id
                                .flatten()
                                .ok_or_else(|| unsupported("client ToolSearch call_id required"))?,
                            item.id.flatten(),
                            item.arguments,
                        )?
                    }
                    r::InputItem::ToolSearchOutput(item) => {
                        if item.execution != Some(r::ToolExecution::Client) {
                            return Err(unsupported(
                                "server ToolSearch result cannot become a client result",
                            ));
                        }
                        result(
                            item.call_id.flatten().ok_or_else(|| {
                                unsupported("client ToolSearch output call_id required")
                            })?,
                            json!({"tools":item.tools.into_declared()}),
                        )?
                    }
                    r::InputItem::AdditionalTools(_) => {
                        report.changed(
                            "input.additional_tools",
                            "declared additional tools are included in the target tool catalog",
                        );
                        return Ok(None);
                    }
                    item => item,
                }))
            })();

            if let Some(mapped) = crate::transform::optional(mapped)?.flatten() {
                lowered.push(mapped);
            }
        }
        *items = lowered;
        Ok(())
    }
}
