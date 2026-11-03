use super::*;

pub fn render(
    request: &GenerateRequest,
    streaming: bool,
) -> Result<Rendered<q::GenerateContentRequestBody>> {
    crate::assets::resolve::validate_request(request)?;
    let opts = &request.provider_options.openai.responses;
    let mut body = q::GenerateContentRequestBody::builder().build();
    body.reasoning = opts.reasoning.clone();
    body.include = opts.include.clone();
    body.store = opts.store;
    body.background = opts.background;
    body.parallel_tool_calls = opts.parallel_tool_calls;
    body.previous_response_id = opts.previous_response_id.clone();
    body.prompt_cache_key = opts.prompt_cache_key.clone();
    body.prompt_cache_options = opts.prompt_cache_options.clone();
    body.prompt_cache_retention = opts.prompt_cache_retention;
    body.service_tier = opts.service_tier;
    body.text = opts.text.clone();
    body.tools = opts.tools.clone();
    body.truncation = opts.truncation;
    body.rest = request.provider_options.openai.rest.clone();
    body.rest.extend(opts.rest.clone());
    body.model = Some(request.model.id.clone());
    body.stream = Some(Some(streaming));
    let input = render_messages(&request.messages)?;

    body.input = Some(i::Input::Items(input));
    if request
        .settings
        .output_modalities
        .contains(&Modality::Audio)
        || request.settings.speech.is_some()
    {
        return Err(Error::invalid(
            "Responses audio output requires the Chat or Realtime dialect",
        ));
    }
    if request
        .settings
        .output_modalities
        .contains(&Modality::Image)
    {
        let tools = body.tools.get_or_insert_with(Vec::new);
        if !tools
            .iter()
            .any(|tool| matches!(tool, t::Tool::ImageGeneration(_)))
        {
            tools.push(t::Tool::ImageGeneration(
                t::ImageGenerationTool::builder().build(),
            ));
        }
    }

    if let Some(value) = request.settings.temperature {
        body.temperature = Some(Some(
            serde_json::Number::from_f64(value)
                .ok_or_else(|| Error::invalid("temperature must be finite"))?,
        ));
    }
    if let Some(value) = request.settings.top_p {
        body.top_p = Some(Some(
            serde_json::Number::from_f64(value)
                .ok_or_else(|| Error::invalid("top_p must be finite"))?,
        ));
    }
    if let Some(value) = request.settings.max_output_tokens {
        body.max_output_tokens = Some(Some(value.into()));
    }
    if !request.settings.stop_sequences.is_empty() {
        return Err(Error::invalid("Responses has no stop-sequence parameter"));
    }
    if !request.tools.is_empty() {
        let functions = request
            .tools
            .iter()
            .map(|tool| {
                let parameters =
                    serde_json::from_value(tool.input_schema.clone()).map_err(Error::invalid)?;
                Ok(t::Tool::Function(
                    t::FunctionTool::builder(tool.name.clone(), Some(parameters), None)
                        .description(Some(tool.description.clone()))
                        .build(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        body.tools.get_or_insert_with(Vec::new).extend(functions);
    }
    if let Some(choice) = &request.settings.tool_choice {
        body.tool_choice = match choice {
            ToolChoice::Auto => Some(i::ToolChoice::Mode(i::ToolChoiceMode::Auto)),
            ToolChoice::None => Some(i::ToolChoice::Mode(i::ToolChoiceMode::None)),
            ToolChoice::Required => Some(i::ToolChoice::Mode(i::ToolChoiceMode::Required)),
            ToolChoice::Tool { name } => Some(i::ToolChoice::Function(
                i::ToolChoiceFunction::builder(
                    i::ToolChoiceFunctionType::ToolChoiceFunction,
                    name.clone(),
                )
                .build(),
            )),
            ToolChoice::Unknown(_) => None,
        };
    }
    if let Some(schema) = &request.settings.output_schema
        && opts.text.as_ref().and_then(|c| c.format.as_ref()).is_none()
    {
        let schema = serde_json::from_value(schema.clone()).map_err(Error::invalid)?;
        let text = body
            .text
            .get_or_insert_with(|| i::TextConfig::builder().build());
        let mut format = match text.format.take() {
            Some(i::TextFormat::JsonSchema(format)) => format,
            Some(i::TextFormat::Text(format)) => {
                i::TextFormatJsonSchema::builder("response".into(), Rest::new())
                    .rest(format.rest)
                    .build()
            }
            Some(i::TextFormat::JsonObject(format)) => {
                i::TextFormatJsonSchema::builder("response".into(), Rest::new())
                    .rest(format.rest)
                    .build()
            }
            None => i::TextFormatJsonSchema::builder("response".into(), Rest::new()).build(),
        };
        format.name = "response".into();
        format.schema = schema;
        text.format = Some(i::TextFormat::JsonSchema(format));
    }
    let mut warnings = crate::dialects::shared::warnings::for_request(request, None)?;
    warnings.extend(crate::dialects::shared::reasoning::openai(
        request, &mut body,
    ));
    Ok(Rendered {
        request: body,
        warnings,
    })
}
mod assets;
use assets::*;

mod messages;
use messages::*;

pub(crate) use assets::is_output_image;

fn render_text(
    block: &Block,
    text: &Text,
    role: &Role,
    pending: &mut Option<Pending>,
    input: &mut Vec<i::InputItem>,
) -> Result<()> {
    let meta = read_metadata::<PartMetadata>(block, Vendor::Openai)?;
    if let Some(PartMetadata {
        native: Some(native),
        item,
        group,
    }) = meta
        && *role == Role::Assistant
    {
        let part = if text.refusal {
            i::OutputContent::Refusal(
                i::ResponseOutputRefusal::builder(
                    i::ResponseOutputRefusalType::ResponseOutputRefusal,
                    text.text.clone(),
                )
                .rest(native.rest)
                .build(),
            )
        } else {
            let annotations = text
                .citations
                .iter()
                .map(citations::render_responses)
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect();
            i::OutputContent::Text(
                i::ResponseOutputText::builder(
                    i::ResponseOutputTextType::ResponseOutputText,
                    text.text.clone(),
                    annotations,
                    native.logprobs,
                )
                .rest(native.rest)
                .build(),
            )
        };
        push_output(
            pending,
            input,
            PartMetadata {
                native: None,
                item,
                group,
            },
            part,
        )?;
    } else if *role == Role::Assistant {
        // Without a native output-message envelope (other vendor, or built by the
        // application), assistant text uses the easy-message form: Responses
        // rejects `input_text` parts on assistant messages.
        flush(pending.take(), input);
        input.push(i::InputItem::Easy(
            i::EasyInputMessage::builder(
                i::MessageContent::Text(text.text.clone()),
                i::MessageRole::Assistant,
            )
            .type_(i::MessageType::Message)
            .build(),
        ));
    } else {
        let mut text = i::ResponseInputText::builder(
            i::ResponseInputTextType::ResponseInputText,
            text.text.clone(),
        )
        .build();
        text.prompt_cache_breakpoint = media::responses_cache(block)?;
        push_input(
            pending,
            input,
            message_role(role)?,
            i::InputContent::Text(text),
        );
    }

    Ok(())
}

fn restore_server(block: &Block, call: &ToolCall) -> Result<Option<i::InputItem>> {
    let Some(meta) = read_native::<ServerMetadata>(block, Vendor::Openai)? else {
        return Ok(None);
    };
    Ok(Some(match meta {
        ServerMetadata::Web {
            status,
            agent,
            rest,
        } => {
            let mut v = i::WebSearchCall::builder(
                i::WebSearchCallType::WebSearchCall,
                call.id.clone(),
                status,
            )
            .rest(rest)
            .build();
            v.agent = agent;
            v.action = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            i::InputItem::WebSearchCall(v)
        }
        ServerMetadata::Files {
            status,
            agent,
            rest,
        } => {
            let queries = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            let mut v = i::FileSearchCall::builder(
                i::FileSearchCallType::FileSearchCall,
                call.id.clone(),
                queries,
                status,
            )
            .rest(rest)
            .build();
            v.agent = agent;
            i::InputItem::FileSearchCall(v)
        }
        ServerMetadata::Code {
            status,
            container_id,
            agent,
            rest,
        } => {
            let code = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            let mut v = i::CodeInterpreterCall::builder(
                i::CodeInterpreterCallType::CodeInterpreterCall,
                call.id.clone(),
                code,
                container_id,
                None,
                status,
            )
            .rest(rest)
            .build();
            v.agent = agent;
            i::InputItem::CodeInterpreterCall(v)
        }
        ServerMetadata::Computer {
            id,
            status,
            pending_safety_checks,
            agent,
            rest,
        } => {
            let mut v = i::ComputerCall::builder(
                i::ComputerCallType::ComputerCall,
                id,
                call.id.clone(),
                pending_safety_checks,
                status,
            )
            .rest(rest)
            .build();
            v.agent = agent;
            let arguments: ComputerArguments =
                serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            v.action = arguments.action;
            v.actions = arguments.actions;
            i::InputItem::ComputerCall(v)
        }
    }))
}
