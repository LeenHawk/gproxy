use crate::{
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{DeclaredFields, gemini as g, openai::chat as c},
};
use std::collections::BTreeMap;
pub fn gemini_to_openai_request(
    input: g::GenerateContentRequestBody,
    target_model: impl Into<String>,
    identity: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<c::GenerateContentRequestBody>, TransformError> {
    if policy.dialect != crate::Dialect::OpenAiChat {
        return Err(TransformError::shape(
            "identity.policy",
            "expected Chat target policy",
        ));
    }
    let model = target_model.into();
    if model.is_empty() {
        return Err(TransformError::missing_metadata("target_model"));
    }
    let input = input.into_declared();
    let mut ids = identity.clone();
    let mut report = Report::default();
    let mut out = c::GenerateContentRequestBody::builder(Vec::new(), model).build();
    super::config::to_chat(&input, &mut out, &mut report)?;
    out.tools = super::tools::gemini_tools_to_chat(input.tools.as_ref(), &mut report)?;
    if input
        .tool_config
        .as_ref()
        .and_then(|config| config.function_calling_config.as_ref())
        .and_then(|config| config.mode.as_ref())
        == Some(&g::FunctionCallingMode::Validated)
    {
        for tool in out.tools.iter_mut().flatten() {
            match tool {
                c::ChatTool::Function(tool) => tool.function.strict = Some(Some(true)),
                c::ChatTool::Custom(_) => {
                    unreachable!("Gemini functions map only to Chat functions")
                }
            }
        }
    }
    let mut bindings = super::content::Calls::default();
    if let Some(mut system) = input.system_instruction {
        system.role = Some("system".into());
        out.messages.extend(super::content::gemini_content_to_chat(
            system,
            &mut report,
            &mut ids,
            policy,
            &mut bindings,
        )?);
    }
    for content in input.contents {
        out.messages.extend(super::content::gemini_content_to_chat(
            content,
            &mut report,
            &mut ids,
            policy,
            &mut bindings,
        )?);
    }
    *identity = ids;
    Ok(Converted { value: out, report })
}
/// The selected Gemini model is an HTTP path resource, not a field of this body.
pub fn openai_to_gemini_request(
    input: &c::GenerateContentRequestBody,
    target_model: impl Into<String>,
    function_names: &BTreeMap<String, String>,
) -> Result<Converted<g::GenerateContentRequestBody>, TransformError> {
    if target_model.into().is_empty() {
        return Err(TransformError::missing_metadata("target_model"));
    }
    let mut names = function_names.clone();
    for message in &input.messages {
        if let c::ChatMessage::Assistant(message) = message {
            for call in message.tool_calls.iter().flatten() {
                if let c::MessageToolCall::Function(call) = call {
                    if names
                        .get(&call.id)
                        .is_some_and(|v| v != &call.function.name)
                    {
                        return Err(TransformError::shape(
                            "function_names",
                            "supplied function name conflicts with history",
                        ));
                    }
                    names.insert(call.id.clone(), call.function.name.clone());
                }
            }
        }
    }
    let mut out = g::GenerateContentRequestBody::builder(Vec::new()).build();
    let mut report = Report::default();
    super::config::to_gemini(input, &mut out, &mut report)?;
    if input.tools.is_some() && input.functions.is_some() {
        return Err(TransformError::shape(
            "tools",
            "legacy/current declarations conflict",
        ));
    }
    out.tools = if let Some(functions) = &input.functions {
        Some(super::tools::legacy(functions))
    } else {
        super::tools::chat_tools_to_gemini(input.tools.as_ref(), &mut report)?
    };
    let strict=input.tools.as_ref().is_some_and(|tools|tools.iter().any(|tool|matches!(tool,c::ChatTool::Function(tool) if tool.function.strict.flatten()==Some(true))));
    if strict {
        let config = out
            .tool_config
            .get_or_insert_with(|| g::ToolConfig::builder().build());
        let function = config
            .function_calling_config
            .get_or_insert_with(|| g::FunctionCallingConfig::builder().build());
        if function.mode.is_none() || function.mode == Some(g::FunctionCallingMode::Auto) {
            function.mode = Some(g::FunctionCallingMode::Validated);
        }
    }
    let mut system = Vec::new();
    for message in &input.messages {
        let (role, parts) = match message {
            c::ChatMessage::System(message) => {
                system.push(
                    g::Part::builder()
                        .text(super::content::chat_text(&message.content))
                        .build(),
                );
                continue;
            }
            c::ChatMessage::Developer(message) => {
                system.push(
                    g::Part::builder()
                        .text(super::content::chat_text(&message.content))
                        .build(),
                );
                continue;
            }
            c::ChatMessage::User(message) => ("user", super::media::user(&message.content)?),
            c::ChatMessage::Assistant(message) => {
                ("model", super::content::assistant(message, &mut report)?)
            }
            c::ChatMessage::Tool(message) => {
                let name = names
                    .get(&message.tool_call_id)
                    .ok_or_else(|| TransformError::missing_metadata("function_response.name"))?;
                let mut response = serde_json::Map::new();
                response.insert(
                    "output".into(),
                    super::content::chat_text(&message.content).into(),
                );
                (
                    "user",
                    vec![
                        g::Part::builder()
                            .function_response(
                                g::FunctionResponse::builder(name.clone(), response)
                                    .id(message.tool_call_id.clone())
                                    .build(),
                            )
                            .build(),
                    ],
                )
            }
            c::ChatMessage::Function(message) => {
                let mut response = serde_json::Map::new();
                response.insert(
                    "output".into(),
                    message
                        .content
                        .as_ref()
                        .map_or(serde_json::Value::Null, |text| text.clone().into()),
                );
                (
                    "user",
                    vec![
                        g::Part::builder()
                            .function_response(
                                g::FunctionResponse::builder(message.name.clone(), response)
                                    .build(),
                            )
                            .build(),
                    ],
                )
            }
        };
        out.contents
            .push(g::Content::builder().parts(parts).role(role).build());
    }
    if !system.is_empty() {
        out.system_instruction = Some(g::Content::builder().parts(system).build());
    }
    Ok(Converted { value: out, report })
}
