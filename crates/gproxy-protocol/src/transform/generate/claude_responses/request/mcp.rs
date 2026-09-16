use crate::{
    transform::{Report, TransformError},
    wire::{claude::count_tokens as c, openai::responses::tools as r},
};
/// Claude's MCP URL connector executes server calls directly and has no
/// approval-request/result wire turn; preserve that contract explicitly.
pub(crate) fn to_responses(
    input: c::McpServerUrlDefinition,
) -> Result<Option<r::Tool>, TransformError> {
    if input.name.is_empty() || input.url.is_empty() {
        return Err(TransformError::shape("mcp_server", "name and URL required"));
    }
    if input
        .tool_configuration
        .as_ref()
        .is_some_and(|v| v.enabled == Some(false))
    {
        return Ok(None);
    }
    let mut out = r::McpTool::builder(input.name).build();
    out.server_url = Some(input.url);
    out.authorization = input.authorization_token;
    out.allowed_tools = input
        .tool_configuration
        .and_then(|v| v.allowed_tools)
        .map(|names| Some(r::McpAllowedTools::Names(names)));
    out.require_approval = Some(Some(r::McpApproval::Setting(r::McpApprovalSetting::Never)));
    Ok(Some(r::Tool::Mcp(out)))
}
pub(crate) fn to_claude(
    input: r::McpTool,
    report: &mut Report,
) -> Result<c::McpServerUrlDefinition, TransformError> {
    let names = match input.allowed_tools.flatten() {
        None => None,
        Some(r::McpAllowedTools::Names(names)) => Some(names),
        Some(r::McpAllowedTools::Filter(filter)) => filter.tool_names,
    };
    let mut out = c::McpServerUrlDefinition::builder(
        input.server_label,
        c::McpServerType::Url,
        input
            .server_url
            .filter(|url| !url.is_empty())
            .ok_or_else(|| TransformError::missing_metadata("mcp.server_url"))?,
    )
    .build();
    out.authorization_token = input.authorization;
    if names.is_some() {
        let mut config = c::McpToolConfiguration::builder().build();
        config.allowed_tools = names;
        out.tool_configuration = Some(config);
    }
    if input.server_description.is_some() {
        report.omitted(
            "mcp.server_description",
            "Claude connector has no description field",
        );
    }
    Ok(out)
}
