//! Responses client-executed tools carried by target-native function calls.
//! The original declared request is the reverse binding; no private wire fields
//! or execution in the proxy are used.

mod history;
mod output;
use crate::{
    transform::{Report, TransformError},
    wire::{DeclaredFields, openai::responses as r},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const SHELL: &str = "gproxy_client_shell";

pub(crate) const PATCH: &str = "gproxy_client_apply_patch";

pub(crate) const SEARCH: &str = "gproxy_client_tool_search";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Namespace { namespace: String, name: String },
    Custom { name: String },
    Shell,
    Patch,
    Search,
}

#[derive(Default, Clone, Debug)]
pub(crate) struct Bindings {
    pub entries: BTreeMap<String, Kind>,
    tools: Vec<r::Tool>,
    hidden: BTreeSet<String>,
    target: Option<crate::Dialect>,
    keep_deferred: bool,
}

pub(crate) fn qualified(namespace: &str, name: &str) -> String {
    // Stable bounded spelling. Exact binding/collision checks below remain the
    // authority; the digest is never decoded as proof of a namespace.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in namespace.bytes().chain([0]).chain(name.bytes()) {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    format!("gproxy_namespace_{hash:016x}")
}

pub(crate) fn custom_alias(name: &str) -> String {
    qualified("custom", name).replacen("gproxy_namespace_", "gproxy_custom_", 1)
}

fn unsupported(detail: &str) -> TransformError {
    TransformError::unsupported("client_tools", detail)
}

fn direct(callers: &Option<Option<Vec<r::AllowedCaller>>>) -> Result<(), TransformError> {
    if callers.as_ref().and_then(Option::as_ref).is_some_and(|v| {
        v.iter()
            .any(|v| matches!(v, r::AllowedCaller::Programmatic))
    }) {
        return Err(unsupported(
            "programmatic execution cannot become client execution",
        ));
    }
    Ok(())
}

fn function(name: &str, description: &str, schema: Value) -> Result<r::Tool, TransformError> {
    let parameters = schema
        .as_object()
        .cloned()
        .ok_or_else(|| unsupported("function schema must be an object"))?;
    let mut out = r::FunctionTool::builder(name.to_owned(), Some(parameters), Some(false)).build();
    out.description = Some(Some(description.into()));
    Ok(r::Tool::Function(out))
}

impl Bindings {
    fn bind(&mut self, alias: String, kind: Kind) -> Result<(), TransformError> {
        if self.entries.get(&alias).is_some_and(|old| old != &kind) {
            return Err(unsupported("tool alias collision"));
        }
        self.entries.insert(alias, kind);
        Ok(())
    }
    pub(crate) fn new(input: &r::GenerateContentRequestBody) -> Result<Self, TransformError> {
        Self::for_target(input, crate::Dialect::OpenAiChat)
    }
    pub(crate) fn for_target(
        input: &r::GenerateContentRequestBody,
        target: crate::Dialect,
    ) -> Result<Self, TransformError> {
        Self::for_parts(&input.tools, &input.input, target)
    }
    pub(crate) fn for_parts(
        tools: &Option<Vec<r::Tool>>,
        history: &Option<r::Input>,
        target: crate::Dialect,
    ) -> Result<Self, TransformError> {
        let discovery = tools.iter().flatten().any(|tool| matches!(tool, r::Tool::ToolSearch(tool) if tool.execution == Some(r::ToolExecution::Client)))
            || matches!(history, Some(r::Input::Items(items)) if items.iter().any(|item| matches!(item, r::InputItem::ToolSearchOutput(_) | r::InputItem::AdditionalTools(_))));
        let mut out = Self {
            target: Some(target),
            keep_deferred: target != crate::Dialect::OpenAiChat && !discovery,
            ..Self::default()
        };
        let mut discovered = Vec::new();
        if let Some(r::Input::Items(items)) = history {
            for item in items {
                match item {
                    r::InputItem::ToolSearchOutput(item)
                        if item.execution == Some(r::ToolExecution::Client) =>
                    {
                        discovered.extend(item.tools.clone())
                    }
                    r::InputItem::AdditionalTools(item) => discovered.extend(item.tools.clone()),
                    _ => {}
                }
            }
        }
        let mut active = BTreeSet::new();
        for tool in &discovered {
            match tool {
                r::Tool::Function(tool) => {
                    active.insert(tool.name.clone());
                }
                r::Tool::Custom(tool) => {
                    active.insert(custom_alias(&tool.name));
                }
                r::Tool::Namespace(ns) => {
                    for tool in &ns.tools {
                        if let r::NamespaceToolDefinition::Function(tool) = tool {
                            active.insert(qualified(&ns.name, &tool.name));
                        }
                    }
                }
                _ => {}
            }
        }
        let all_tools: Vec<_> = tools
            .clone()
            .unwrap_or_default()
            .into_iter()
            .chain(discovered)
            .collect();
        for tool in all_tools.iter().cloned() {
            crate::transform::optional(out.add(tool.into_declared(), &active))?;
        }
        let mut definitions = BTreeMap::new();
        for tool in std::mem::take(&mut out.tools) {
            let name = match &tool {
                r::Tool::Function(tool) => Some(tool.name.clone()),
                r::Tool::Custom(tool) => Some(tool.name.clone()),
                _ => None,
            };
            if let Some(name) = name
                && let Some(previous) = definitions.insert(name, tool.clone())
            {
                if previous != tool {
                    return Err(unsupported("conflicting definitions for the same tool"));
                }
                continue;
            }
            out.tools.push(tool);
        }
        // A client may legitimately declare any ordinary function name. Do not
        // let it masquerade as a generated native-tool binding.
        for tool in &all_tools {
            let name = match tool {
                r::Tool::Function(t) => Some(&t.name),
                r::Tool::Custom(t) => Some(&t.name),
                _ => None,
            };
            if name.is_some_and(|name| out.entries.contains_key(name)) {
                return Err(unsupported("declared tool collides with client-tool alias"));
            }
        }
        Ok(out)
    }
    fn add(&mut self, tool: r::Tool, active: &BTreeSet<String>) -> Result<(), TransformError> {
        match tool {
            r::Tool::Custom(tool)
                if matches!(
                    self.target,
                    Some(crate::Dialect::Claude | crate::Dialect::Gemini)
                ) =>
            {
                direct(&tool.allowed_callers)?;
                let alias = custom_alias(&tool.name);
                self.bind(
                    alias.clone(),
                    Kind::Custom {
                        name: tool.name.clone(),
                    },
                )?;
                if tool.defer_loading == Some(true)
                    && !self.keep_deferred
                    && !active.contains(&alias)
                {
                    self.hidden.insert(alias);
                    return Ok(());
                }
                let description = format!(
                    "Execute the client's {} tool. Put its complete raw tool input in the input string.\nTool instructions: {}\nInput format: {}",
                    tool.name,
                    tool.description.unwrap_or_default(),
                    serde_json::to_string(&tool.format)?,
                );
                self.tools.push(function(
                    &alias,
                    &description,
                    json!({
                        "type":"object", "properties":{"input":{"type":"string"}},
                        "required":["input"], "additionalProperties":false
                    }),
                )?);
            }
            r::Tool::Namespace(ns) => {
                if ns.name.is_empty() {
                    return Err(unsupported("empty namespace"));
                }
                for tool in ns.tools {
                    let r::NamespaceToolDefinition::Function(tool) = tool else {
                        continue;
                    };
                    direct(&tool.allowed_callers)?;
                    if tool.name.is_empty() {
                        return Err(unsupported("empty namespaced function name"));
                    }
                    if tool
                        .output_schema
                        .as_ref()
                        .and_then(Option::as_ref)
                        .is_some()
                        && self.target != Some(crate::Dialect::Gemini)
                    {
                        return Err(unsupported(
                            "selected backend cannot enforce tool output schema",
                        ));
                    }
                    let alias = qualified(&ns.name, &tool.name);
                    self.bind(
                        alias.clone(),
                        Kind::Namespace {
                            namespace: ns.name.clone(),
                            name: tool.name,
                        },
                    )?;
                    if tool.defer_loading == Some(true)
                        && !self.keep_deferred
                        && !active.contains(&alias)
                    {
                        self.hidden.insert(alias);
                        continue;
                    }
                    let parameters = tool
                        .parameters
                        .flatten()
                        .map(|v| {
                            v.as_object().cloned().ok_or_else(|| {
                                unsupported("namespace function schema must be an object")
                            })
                        })
                        .map(crate::transform::optional)
                        .transpose()?
                        .flatten();
                    let mut target =
                        r::FunctionTool::builder(alias, parameters, tool.strict.flatten()).build();
                    target.output_schema = tool.output_schema.filter(Option::is_some);
                    target.defer_loading =
                        self.keep_deferred.then_some(tool.defer_loading).flatten();
                    target.description = Some(Some(format!(
                        "{}\n{}",
                        ns.description,
                        tool.description.flatten().unwrap_or_default()
                    )));
                    self.tools.push(r::Tool::Function(target));
                }
            }
            r::Tool::Shell(tool) => {
                direct(&tool.allowed_callers)?;
                match tool.environment.flatten() {
                    None => {}
                    Some(r::ShellEnvironment::Local(local))
                        if local.skills.as_ref().is_none_or(Vec::is_empty) => {}
                    _ => {
                        return Err(unsupported(
                            "hosted shell environments and skills require their original execution environment",
                        ));
                    }
                }
                self.bind(SHELL.into(), Kind::Shell)?;
                self.tools.push(function(SHELL, "Run commands in the client's local shell. The client executes the action and returns stdout, stderr and exit status.", json!({"type":"object","properties":{"commands":{"type":"array","items":{"type":"string"}},"timeout_ms":{"type":["integer","null"]},"max_output_length":{"type":["integer","null"]}},"required":["commands"],"additionalProperties":false}))?);
            }
            r::Tool::ApplyPatch(tool) => {
                direct(&tool.allowed_callers)?;
                self.bind(PATCH.into(), Kind::Patch)?;
                self.tools.push(function(PATCH, "Apply a file operation on the client. Return a create_file, update_file or delete_file operation; create/update use the apply_patch diff format.", json!({"type":"object","properties":{"type":{"type":"string","enum":["create_file","update_file","delete_file"]},"path":{"type":"string"},"diff":{"type":"string"}},"required":["type","path"],"additionalProperties":false}))?);
            }
            r::Tool::ToolSearch(tool) => {
                if tool.execution != Some(r::ToolExecution::Client) {
                    return Err(unsupported(
                        "server-side ToolSearch requires its native executor",
                    ));
                }
                let schema = tool.parameters.flatten().ok_or_else(|| {
                    TransformError::missing_metadata("client ToolSearch parameters")
                })?;
                self.bind(SEARCH.into(), Kind::Search)?;
                self.tools.push(function(
                    SEARCH,
                    tool.description
                        .flatten()
                        .as_deref()
                        .unwrap_or("Discover tools by calling the client's tool search."),
                    schema,
                )?);
            }
            r::Tool::Function(mut tool)
                if tool.defer_loading == Some(true) && !self.keep_deferred =>
            {
                direct(&tool.allowed_callers)?;
                if active.contains(&tool.name) {
                    tool.defer_loading = None;
                    self.tools.push(r::Tool::Function(tool));
                } else {
                    self.hidden.insert(tool.name);
                }
            }
            tool => self.tools.push(tool),
        }
        Ok(())
    }
    pub(crate) fn lower(
        &self,
        input: &mut r::GenerateContentRequestBody,
        report: &mut Report,
    ) -> Result<(), TransformError> {
        self.lower_parts(
            &mut input.tools,
            &mut input.input,
            &mut input.tool_choice,
            report,
        )
    }
    pub(crate) fn lower_parts(
        &self,
        tools: &mut Option<Vec<r::Tool>>,
        history: &mut Option<r::Input>,
        choice: &mut Option<r::ToolChoice>,
        report: &mut Report,
    ) -> Result<(), TransformError> {
        if tools.is_some() || !self.tools.is_empty() {
            *tools = Some(self.tools.clone());
        }
        if !self.hidden.is_empty() {
            report.changed("tools.defer_loading", "undiscovered deferred tools stay out of the target catalog until declared discovery output activates them");
        }
        self.history(history, report)?;
        let selection = (|| -> Result<(), TransformError> {
            if let Some(choice) = choice {
                if matches!(choice, r::ToolChoice::Function(tool) if self.hidden.contains(&tool.name))
                {
                    return Err(unsupported(
                        "selected deferred tool has not been discovered",
                    ));
                }
                if let r::ToolChoice::Allowed(allowed) = choice {
                    for selector in &mut allowed.tools {
                        let alias = match selector.get("type").and_then(Value::as_str) {
                            Some("shell") => Some(SHELL.to_owned()),
                            Some("apply_patch") => Some(PATCH.to_owned()),
                            Some("tool_search") => Some(SEARCH.to_owned()),
                            Some("function") => match (
                                selector.get("namespace").and_then(Value::as_str),
                                selector.get("name").and_then(Value::as_str),
                            ) {
                                (Some(namespace), Some(name)) => {
                                    Some(self.namespace_name(namespace, name)?)
                                }
                                _ => None,
                            },
                            _ => None,
                        };
                        if let Some(alias) = alias {
                            if !self
                                .tools
                                .iter()
                                .any(|tool| matches!(tool, r::Tool::Function(t) if t.name == alias))
                            {
                                return Err(unsupported(
                                    "selected tool is not active in the declared catalog",
                                ));
                            }
                            *selector = serde_json::Map::from_iter([
                                ("type".into(), json!("function")),
                                ("name".into(), json!(alias)),
                            ]);
                        }
                    }
                    if self.target == Some(crate::Dialect::Claude) {
                        let names = allowed
                            .tools
                            .iter()
                            .map(|selector| {
                                if selector.get("type").and_then(Value::as_str) != Some("function")
                                {
                                    return Err(unsupported(
                                        "selected subset contains a non-function tool",
                                    ));
                                }
                                selector
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .map(str::to_owned)
                                    .ok_or_else(|| unsupported("selected function name required"))
                            })
                            .filter_map(|value| crate::transform::optional(value).transpose())
                            .collect::<Result<BTreeSet<_>, _>>()?;
                        if names.is_empty()
                            || names.iter().any(|name| {
                                !self.tools.iter().any(
                                    |tool| matches!(tool, r::Tool::Function(t) if &t.name == name),
                                )
                            })
                        {
                            return Err(unsupported(
                                "selected function is not active in the catalog",
                            ));
                        }
                        *tools = Some(self.tools.iter().filter(|tool| matches!(tool, r::Tool::Function(t) if names.contains(&t.name))).cloned().collect());
                        *choice = r::ToolChoice::Mode(match allowed.mode {
                            r::AllowedToolChoiceMode::Auto => r::ToolChoiceMode::Auto,
                            r::AllowedToolChoiceMode::Required => r::ToolChoiceMode::Required,
                        });
                        report.changed("tool_choice.allowed_tools", "target catalog restricted to the selected functions; selection mode preserved");
                    }
                }
                let alias = match choice {
                    r::ToolChoice::Shell(_) => Some(SHELL.to_owned()),
                    r::ToolChoice::ApplyPatch(_) => Some(PATCH.to_owned()),
                    r::ToolChoice::Custom(tool)
                        if self.entries.contains_key(&custom_alias(&tool.name)) =>
                    {
                        Some(custom_alias(&tool.name))
                    }
                    _ => None,
                };
                if let Some(alias) = alias {
                    if !self.entries.contains_key(&alias) {
                        return Err(unsupported("selected client tool was not declared"));
                    }
                    *choice = r::ToolChoice::Function(
                        r::ToolChoiceFunction::builder(
                            r::ToolChoiceFunctionType::ToolChoiceFunction,
                            alias,
                        )
                        .build(),
                    );
                }
            }
            Ok(())
        })();
        if crate::transform::optional(selection)?.is_none() {
            *choice = None;
            *tools = Some(self.tools.clone());
            report.omitted("tool_choice", "selection has no target representation");
        }
        if !self.entries.is_empty() {
            report.changed("tools", "client-executed tools use bound functions; original call types are restored on return");
        }
        Ok(())
    }
}
