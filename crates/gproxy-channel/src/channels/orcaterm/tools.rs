//! Bind OrcaTerm's registered UI tools to tools declared by the API client.
//! Calls run in the client with its normal permission checks; the native
//! review and conversation are retained for reportToolResult.
use super::{
    config::{invalid, new_id, now_ms},
    request,
};
use crate::channel::{ChannelError, ChannelState};
use gproxy_protocol::capability::{CasResult, StateWrite};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime},
};

const UI_SERVER: &str = "ui-tools-orcaterm-explorer";
const TERMINAL: &str = "gproxy-client";
const TTL: u64 = 24 * 60 * 60;

#[derive(Clone)]
enum Kind {
    Shell {
        command: &'static str,
        cwd: Option<&'static str>,
    },
    Read {
        path: &'static str,
    },
    Edit,
    AgyEdit,
    Patch,
    Write {
        path: &'static str,
        content: &'static str,
    },
    Fetch,
    AgyFetch,
    Web,
}
#[derive(Clone)]
struct Binding {
    name: String,
    kind: Kind,
    schema: Value,
}

#[derive(Default)]
pub(super) struct Tools {
    bindings: BTreeMap<&'static str, Binding>,
    cwd: Option<String>,
    context: Option<Context>,
    files: BTreeMap<String, u64>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Context {
    conversation_id: String,
    account: String,
    model: String,
    origin: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Pending {
    context: Context,
    tool: Value,
    input_id: String,
    files: BTreeMap<String, u64>,
}

fn key(id: &str) -> String {
    format!("orcaterm:client-tool:v1:{id}")
}
fn expires() -> SystemTime {
    SystemTime::UNIX_EPOCH
        + Duration::from_millis(now_ms().max(0) as u64)
        + Duration::from_secs(TTL)
}
fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    value[name]
        .as_str()
        .ok_or_else(|| invalid(format!("native tool has no {name}")))
}
fn quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

impl Tools {
    pub fn from_request(input: &Value) -> Result<Self, ChannelError> {
        let mut out = Self {
            cwd: working_directory(input),
            ..Default::default()
        };
        if input["tool_choice"] == "none" {
            return Ok(out);
        }
        let mut read = None;
        let mut shell = None;
        let mut edit = None;
        let mut write = None;
        let mut fetch = None;
        for tool in input["tools"].as_array().into_iter().flatten() {
            if tool["type"] != "function" {
                continue;
            }
            let f = &tool["function"];
            let Some(name) = f["name"].as_str().filter(|s| !s.is_empty()) else {
                return Err(request::bad("function tool has no name"));
            };
            let p = &f["parameters"]["properties"];
            let has = |k: &str| p.get(k).is_some();
            let description = f["description"].as_str().unwrap_or_default();
            let kind = if has("cmd")
                && (name == "exec_command" || description.contains("command in a PTY"))
            {
                Some(Kind::Shell {
                    command: "cmd",
                    cwd: has("workdir").then_some("workdir"),
                })
            } else if has("CommandLine") && name == "run_command" {
                Some(Kind::Shell {
                    command: "CommandLine",
                    cwd: Some("Cwd"),
                })
            } else if has("command") && (name == "Bash" || has("run_in_background")) {
                Some(Kind::Shell {
                    command: "command",
                    cwd: None,
                })
            } else if has("file_path") && has("old_string") && has("new_string") {
                Some(Kind::Edit)
            } else if has("TargetFile") && has("TargetContent") && has("ReplacementContent") {
                Some(Kind::AgyEdit)
            } else if has("input")
                && f["description"]
                    .as_str()
                    .is_some_and(|s| s.contains("apply_patch"))
            {
                Some(Kind::Patch)
            } else if has("file_path") && has("content") {
                Some(Kind::Write {
                    path: "file_path",
                    content: "content",
                })
            } else if has("TargetFile") && has("CodeContent") {
                Some(Kind::Write {
                    path: "TargetFile",
                    content: "CodeContent",
                })
            } else if has("file_path") && (has("offset") || has("limit")) {
                Some(Kind::Read { path: "file_path" })
            } else if has("AbsolutePath") {
                Some(Kind::Read {
                    path: "AbsolutePath",
                })
            } else if has("url") && has("prompt") {
                Some(Kind::Fetch)
            } else if has("Url") {
                Some(Kind::AgyFetch)
            } else if has("open") && has("search_query") {
                Some(Kind::Web)
            } else {
                None
            };
            let Some(kind) = kind else {
                continue;
            };
            let binding = Binding {
                name: name.to_owned(),
                kind: kind.clone(),
                schema: f["parameters"].clone(),
            };
            match kind {
                Kind::Shell { .. } => shell = Some(binding),
                Kind::Read { .. } => read = Some(binding),
                Kind::Edit | Kind::AgyEdit | Kind::Patch => edit = Some(binding),
                Kind::Write { .. } => write = Some(binding),
                Kind::Fetch | Kind::AgyFetch | Kind::Web => fetch = Some(binding),
            }
        }
        if let Some(binding) = &shell {
            out.bindings
                .insert("execute_terminal_command", binding.clone());
        }
        if let Some(binding) = read.or(shell) {
            out.bindings.insert("remote_read", binding);
        }
        if let Some(binding) = edit {
            if matches!(binding.kind, Kind::Patch) && write.is_none() {
                write = Some(binding.clone());
            }
            out.bindings.insert("remote_edit", binding);
        }
        if let Some(binding) = write {
            out.bindings.insert("remote_write", binding);
        }
        if let Some(binding) = fetch {
            out.bindings.insert("fetch", binding);
        }
        Ok(out)
    }
    pub fn enabled(&self) -> bool {
        !self.bindings.is_empty()
    }
    pub fn prompt(&self) -> String {
        if !self.enabled() {
            return String::new();
        }
        if self.bindings.len() == 1 && self.bindings.contains_key("fetch") {
            return "Web fetch results for this API request are supplied by the calling client through reportToolResult.".into();
        }
        let names: Vec<_> = self
            .bindings
            .keys()
            .filter(|name| **name != "fetch")
            .copied()
            .collect();
        format!(
            "The attached API client supplies terminal and file operations through OrcaTerm's native UI tool review protocol. For terminal/file operations use action=review with tool.type=ui, tool.uiServer={UI_SERVER}, and one of these tool.name values: {}. Use execute_terminal_command with args.command for shell execution; remote_read with args.file_path to read; remote_edit with args.file_path, old_string and new_string to edit; remote_write with args.file_path and content to write. Each terminal/file call must include args.terminal_id={TERMINAL} and args.connect_config_id={TERMINAL}. The client's working directory is {}. These operations execute in the calling client's filesystem. Use those UI operations for client tools mentioned in the client instructions. Tool results arrive through reportToolResult; wait for each result before continuing. Read a file before editing it. The client enforces its own tool permissions.",
            names.join(", "),
            self.cwd
                .as_deref()
                .unwrap_or("the attached terminal's current directory")
        )
    }
    pub fn configure(&self, body: &mut Value) {
        if !self.enabled() {
            return;
        }
        let ui: Vec<_> = self
            .bindings
            .keys()
            .filter(|name| **name != "fetch")
            .copied()
            .collect();
        if !ui.is_empty() {
            body["user"]["setting"]["uiServers"] = json!([UI_SERVER]);
            body["user"]["setting"]["selectedUITools"] = json!({UI_SERVER:ui});
        } else {
            return;
        }
        let mut terminal = json!({"terminal_id":TERMINAL,"connect_config_id":TERMINAL,"name":"API client terminal",
            "source":"local","execution_target":"local","transport":"client_tools","isLogin":true});
        if let Some(cwd) = &self.cwd {
            terminal["pwd"] = json!(cwd);
        }
        body["user"]["setting"]["env"] = json!([[
            "已登录的终端会话",
            json!({"terminals":[terminal],"activeTerminalId":TERMINAL}).to_string()
        ]]);
    }

    pub async fn prepare(
        &mut self,
        state: &dyn ChannelState,
        body: &mut Value,
        result: Option<(String, String)>,
        origin: String,
    ) -> Result<(), ChannelError> {
        if let Some((call_id, result)) = result {
            let entry = state.get(&key(&call_id)).await?.ok_or_else(|| {
                request::bad("tool continuation expired or belongs to another credential")
            })?;
            let pending: Pending = serde_json::from_slice(&entry.payload)
                .map_err(|_| invalid("invalid cached tool continuation"))?;
            if pending.context.account != body["user"]["id"]
                || pending.context.model != body["user"]["setting"]["model"]
                || pending.context.origin != origin
            {
                return Err(request::bad(
                    "tool continuation account, model or upstream origin changed",
                ));
            }
            self.files = pending.files;
            if pending.tool["name"] == "remote_read" {
                if let Some(n) = line_count(&result) {
                    self.files.insert(self.path(&pending.tool["args"])?, n);
                }
            } else if pending.tool["name"] == "remote_write"
                || (pending.tool["name"] == "remote_edit"
                    && text(&pending.tool["args"], "old_string")?
                        .matches('\n')
                        .count()
                        != text(&pending.tool["args"], "new_string")?
                            .matches('\n')
                            .count())
            {
                // A replacement with the same newline count keeps the read
                // range valid, whether it succeeds or fails. Other edits
                // require a fresh read before a range-based client edit.
                self.files.remove(&self.path(&pending.tool["args"])?);
            }
            body["conversationId"] = json!(pending.context.conversation_id);
            body["input"] = json!({"id":pending.input_id,"type":"review","reviewAction":"reportToolResult",
                "tool":pending.tool,"toolResult":result,"emphasisPrompt":body["input"]["emphasisPrompt"],"files":body["input"]["files"],
                "historyMessages":body["input"]["historyMessages"],"input":body["input"]["input"]});
        }
        self.context = Some(Context {
            conversation_id: text(body, "conversationId")?.to_owned(),
            account: text(&body["user"], "id")?.to_owned(),
            model: text(&body["user"]["setting"], "model")?.to_owned(),
            origin,
        });
        Ok(())
    }

    pub fn review(&self, tool: &Value, message_id: &str) -> Result<(Value, Pending), ChannelError> {
        let name = text(tool, "name")?;
        let binding = self.bindings.get(name).ok_or_else(|| {
            invalid(format!(
                "no declared client tool can execute OrcaTerm {name}"
            ))
        })?;
        if name == "fetch" {
            if tool["type"] != "mcp" || tool["mcpServer"] != "mcp-server-web-fetch" {
                return Err(invalid("unsupported native fetch service"));
            }
        } else if tool["type"] != "ui" || tool["uiServer"].as_str().is_some_and(|s| s != UI_SERVER)
        {
            return Err(invalid("unsupported native client tool service"));
        }
        if tool["args"]["terminal_id"]
            .as_str()
            .is_some_and(|id| id != TERMINAL)
        {
            return Err(invalid("native tool selected an unattached terminal"));
        }
        let context = self
            .context
            .clone()
            .ok_or_else(|| invalid("missing native tool conversation context"))?;
        let arguments = self.arguments(binding, name, &tool["args"])?;
        let digest = Sha256::digest(format!("{}:{message_id}", context.conversation_id).as_bytes());
        let id = format!(
            "call_ot_{}",
            digest[..16]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let call = json!({"id":id,"type":"function","function":{"name":binding.name,"arguments":arguments.to_string()}});
        Ok((
            call,
            Pending {
                context,
                tool: tool.clone(),
                input_id: new_id()?,
                files: self.files.clone(),
            },
        ))
    }

    fn arguments(
        &self,
        binding: &Binding,
        name: &str,
        args: &Value,
    ) -> Result<Value, ChannelError> {
        let path = || self.path(args);
        let mut out = match &binding.kind {
            Kind::Shell { command, cwd } => {
                let cmd = if name == "remote_read" {
                    format!("cat -- {}", quote(&path()?))
                } else {
                    text(args, "command")?.to_owned()
                };
                let mut value = json!({*command:cmd});
                if let Some(cwd_key) = cwd {
                    value[*cwd_key] = json!(self.cwd.as_deref().unwrap_or("."));
                }
                if binding.schema["properties"]
                    .get("WaitMsBeforeAsync")
                    .is_some()
                {
                    value["WaitMsBeforeAsync"] = json!(10000);
                }
                value
            }
            Kind::Read { path: key } => {
                let mut value = json!({*key:path()?});
                if *key == "file_path" {
                    for key in ["offset", "limit"] {
                        if let Some(v) = args.get(key) {
                            value[key] = v.clone();
                        }
                    }
                }
                value
            }
            Kind::Edit => {
                json!({"file_path":path()?,"old_string":text(args,"old_string")?,"new_string":text(args,"new_string")?,"replace_all":args["replace_all"].as_bool().unwrap_or(false)})
            }
            Kind::AgyEdit => {
                let path = path()?;
                let lines = self.files.get(&path).copied().ok_or_else(|| {
                    invalid("read this file before using the client's range-based editor")
                })?;
                json!({"TargetFile":path,"TargetContent":text(args,"old_string")?,"ReplacementContent":text(args,"new_string")?,
                    "AllowMultiple":args["replace_all"].as_bool().unwrap_or(false),"StartLine":1,"EndLine":lines,
                    "Instruction":"Apply the requested exact text replacement","Description":"Edit file contents"})
            }
            Kind::Patch => {
                let path = path()?;
                if path.contains(['\r', '\n']) {
                    return Err(invalid(
                        "patch tool cannot represent a path containing line breaks",
                    ));
                }
                let patch = if name == "remote_write" {
                    format!(
                        "*** Begin Patch\n*** Add File: {path}\n{}*** End Patch",
                        patch_lines('+', text(args, "content")?)
                    )
                } else {
                    if args["replace_all"] == true {
                        return Err(invalid(
                            "the client's patch tool cannot express replace_all without a complete file snapshot",
                        ));
                    }
                    format!(
                        "*** Begin Patch\n*** Update File: {path}\n@@\n{}{}*** End Patch",
                        patch_lines('-', text(args, "old_string")?),
                        patch_lines('+', text(args, "new_string")?)
                    )
                };
                json!({"input":patch})
            }
            Kind::Write { path: key, content } => {
                let mut value = json!({*key:path()?,*content:text(args,"content")?});
                if *key == "TargetFile" {
                    value["Overwrite"] = json!(true);
                    value["Description"] = json!("Write file contents");
                }
                value
            }
            Kind::Fetch => {
                json!({"url":text(args,"url")?,"prompt":"Return the retrieved page contents relevant to the user's request, including its source URL. Do not invent missing page content."})
            }
            Kind::AgyFetch => json!({"Url":text(args,"url")?}),
            Kind::Web => json!({"open":[{"ref_id":text(args,"url")?}],"response_length":"short"}),
        };
        for (key, value) in [
            ("toolSummary", "Client tool execution"),
            ("toolAction", "Executing client tool"),
        ] {
            if binding.schema["properties"].get(key).is_some() {
                out[key] = json!(value);
            }
        }
        Ok(out)
    }

    fn path(&self, args: &Value) -> Result<String, ChannelError> {
        let path = text(args, "file_path")?;
        if path.starts_with('/') || path.as_bytes().get(1) == Some(&b':') {
            Ok(path.to_owned())
        } else {
            Ok(self.cwd.as_ref().map_or_else(
                || path.to_owned(),
                |cwd| format!("{}/{path}", cwd.trim_end_matches('/')),
            ))
        }
    }
}

impl Pending {
    pub async fn save(&self, state: &dyn ChannelState, id: &str) -> Result<(), ChannelError> {
        let key = key(id);
        let write = StateWrite {
            payload: serde_json::to_vec(self)
                .map_err(|_| invalid("could not encode tool continuation"))?
                .into(),
            expires_at: Some(expires()),
        };
        if matches!(
            state.compare_exchange(&key, None, Some(write)).await?,
            CasResult::Applied(_)
        ) {
            return Ok(());
        }
        let old = state
            .get(&key)
            .await?
            .ok_or_else(|| invalid("tool continuation write conflicted"))?;
        let old: Pending = serde_json::from_slice(&old.payload)
            .map_err(|_| invalid("invalid cached tool continuation"))?;
        if old.context.conversation_id != self.context.conversation_id || old.tool != self.tool {
            return Err(invalid("tool continuation identity collision"));
        }
        Ok(())
    }
}

fn patch_lines(prefix: char, text: &str) -> String {
    text.lines()
        .map(|line| format!("{prefix}{line}\n"))
        .collect()
}
fn line_count(text: &str) -> Option<u64> {
    let rest = text.split_once("Total Lines:")?.1.trim_start();
    rest.chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
}
fn working_directory(input: &Value) -> Option<String> {
    for message in input["messages"].as_array().into_iter().flatten().rev() {
        let content = &message["content"];
        let parts: Vec<_> = content
            .as_str()
            .into_iter()
            .chain(
                content
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|p| p["text"].as_str()),
            )
            .collect();
        for text in parts.into_iter().rev() {
            if let Some((_, env)) = text.split_once("<environment_context>")
                && let Some((_, cwd)) = env.split_once("<cwd>")
                && let Some((cwd, _)) = cwd.split_once("</cwd>")
            {
                return Some(cwd.to_owned());
            }
            if !matches!(message["role"].as_str(), Some("system" | "developer")) {
                continue;
            }
            for line in text.lines().rev() {
                for marker in [
                    "Primary working directory:",
                    "Command Working Directory:",
                    "Working directory:",
                ] {
                    if let Some((_, cwd)) = line.split_once(marker) {
                        return Some(cwd.trim().trim_matches('`').to_owned());
                    }
                }
            }
        }
    }
    None
}

/// Strip a native Agent JSON code fence without repairing incomplete JSON.
pub(super) fn json_content(text: &str) -> &str {
    let text = text.trim();
    if text.starts_with('{') || text.starts_with('[') {
        return text;
    }
    let Some((_, fenced)) = text.split_once("```") else {
        return text;
    };
    let Some((language, body)) = fenced.split_once('\n') else {
        return text;
    };
    if !language.trim().is_empty() && !language.trim().eq_ignore_ascii_case("json") {
        return text;
    }
    body.trim().strip_suffix("```").unwrap_or(body).trim()
}
