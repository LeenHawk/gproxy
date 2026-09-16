use crate::{
    transform::{
        Converted, Report, TransformError,
        generate::claude_responses::{self, request as pair},
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{DeclaredFields, claude::count_tokens as c, openai::count_tokens as o},
};

pub fn claude_to_openai(
    input: c::CountTokensRequestBody,
    target_model: impl Into<String>,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<o::CountTokensRequestBody>, TransformError> {
    super::policy(policy, crate::Dialect::OpenAi)?;
    let model = super::model(target_model)?;
    let input = input.into_declared();
    let mut ids = flow.clone();
    let mut report = Report::default();
    let mut out = o::CountTokensRequestBody::builder().build();
    out.model = Some(Some(model.clone()));
    super::controls::claude_to_openai(&input, &mut out, &mut report)?;
    out.tools = input
        .tools
        .map(|tools| pair::tools::to_responses(tools, &mut report))
        .map(crate::transform::optional)
        .transpose()?
        .flatten()
        .map(Some);
    if let Some(servers) = input.mcp_servers {
        for server in servers {
            if let Some(tool) = pair::mcp::to_responses(server)? {
                out.tools
                    .get_or_insert_with(|| Some(Vec::new()))
                    .as_mut()
                    .unwrap()
                    .push(tool);
            }
        }
    }
    if let Some(choice) = input.tool_choice {
        let (choice, parallel) = pair::tools::choice_to_responses(choice);
        out.tool_choice = Some(Some(choice));
        out.parallel_tool_calls = parallel.map(Some);
    }
    out.instructions = input.system.map(|system| {
        Some(match system {
            c::SystemPrompt::Text(text) => text,
            c::SystemPrompt::Blocks(blocks) => blocks
                .into_iter()
                .map(|block| block.text)
                .collect::<Vec<_>>()
                .join("\n"),
        })
    });
    out.input = Some(Some(o::Input::Items(pair::messages::to_responses(
        input.messages,
        &mut ids,
        policy,
        &mut report,
    )?)));
    let mut input = out.input.take().flatten();
    crate::transform::instructions::responses(
        &mut input,
        &mut out.instructions,
        &model,
        &mut report,
    );
    out.input = Some(input);
    *flow = ids;
    Ok(Converted { value: out, report })
}

pub fn openai_to_claude(
    input: o::CountTokensRequestBody,
    target_model: impl Into<String>,
    context: claude_responses::ClaudeRequestContext,
) -> Result<Converted<c::CountTokensRequestBody>, TransformError> {
    let model = super::model(target_model)?;
    let mut input = input.into_declared();
    super::openai_state(&input)?;

    let mut out = c::CountTokensRequestBody::builder(Vec::new(), model).build();
    let mut report = Report::default();
    let mut tools = input.tools.take().flatten();
    let mut history = input.input.take().flatten();
    let mut choice = input.tool_choice.take().flatten();
    crate::transform::generate::client_tools::Bindings::for_parts(
        &tools,
        &history,
        crate::Dialect::Claude,
    )?
    .lower_parts(&mut tools, &mut history, &mut choice, &mut report)?;
    input.tools = tools.map(Some);
    input.input = history.map(Some);
    input.tool_choice = choice.map(Some);
    super::controls::openai_to_claude(&input, &mut out, &mut report)?;
    if let Some(Some(tools)) = input.tools {
        let mut functions = Vec::new();
        let mut servers = Vec::new();
        for tool in tools {
            match tool {
                o::Tool::Mcp(tool) => servers.push(pair::mcp::to_claude(tool, &mut report)?),
                tool => functions.push(tool),
            }
        }
        if !functions.is_empty() {
            out.tools = Some(pair::tools::to_claude(functions)?);
        }
        if !servers.is_empty() {
            out.mcp_servers = Some(servers);
        }
    }
    let choice = input.tool_choice.flatten().or_else(|| {
        input
            .parallel_tool_calls
            .flatten()
            .map(|_| o::ToolChoice::Mode(o::ToolChoiceMode::Auto))
    });
    out.tool_choice = choice
        .map(|choice| pair::tools::choice_to_claude(choice, input.parallel_tool_calls.flatten()))
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let (messages, mut system) =
        pair::history::to_claude(input.input.flatten(), context, &mut report)?;
    out.messages = messages;
    crate::transform::instructions::claude(&mut out.messages, &out.model, &mut report);
    if let Some(Some(text)) = input.instructions {
        system.insert(
            0,
            crate::claude::content::TextBlock::builder(
                crate::claude::content::TextBlockType::Tag,
                text,
            )
            .build(),
        );
    }
    if !system.is_empty() {
        out.system = Some(c::SystemPrompt::Blocks(system));
    }
    Ok(Converted { value: out, report })
}
