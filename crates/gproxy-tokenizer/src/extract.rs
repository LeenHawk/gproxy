use serde_json::Value;

use crate::CountError;

/// JSON request layout, independent of the selected tokenizer vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestFormat {
    OpenAiChat,
    OpenAiResponses,
    Claude,
    Gemini,
}

pub(crate) fn extract(format: RequestFormat, body: &[u8]) -> Result<String, CountError> {
    let root: Value = serde_json::from_slice(body).map_err(CountError::InvalidJson)?;
    if !root.is_object() {
        return Err(CountError::InvalidRequest("expected a JSON object"));
    }
    let mut texts = Texts::default();
    match format {
        RequestFormat::OpenAiChat | RequestFormat::Claude => {
            texts.content(&root["system"]);
            let messages = root["messages"]
                .as_array()
                .ok_or(CountError::InvalidRequest("messages must be an array"))?;
            for message in messages {
                texts.content(message);
            }
            texts.json_fields(
                &root,
                &[
                    "tools",
                    "tool_choice",
                    "functions",
                    "function_call",
                    "response_format",
                    "output_format",
                    "output_config",
                ],
            );
        }
        RequestFormat::OpenAiResponses => {
            texts.content(&root["instructions"]);
            texts.content(&root["input"]);
            texts.json_fields(&root, &["tools", "tool_choice"]);
            texts.json_fields(&root["text"], &["format"]);
        }
        RequestFormat::Gemini => {
            let embedded = root
                .get("generateContentRequest")
                .or_else(|| root.get("generate_content_request"));
            if embedded.is_some() && root.get("contents").is_some() {
                return Err(CountError::InvalidRequest(
                    "contents and generateContentRequest are mutually exclusive",
                ));
            }
            let request = embedded.unwrap_or(&root);
            let messages = request["contents"]
                .as_array()
                .ok_or(CountError::InvalidRequest("contents must be an array"))?;
            texts.content(&request["systemInstruction"]);
            texts.content(&request["system_instruction"]);
            for message in messages {
                texts.content(message);
            }
            texts.json_fields(
                request,
                &[
                    "tools",
                    "toolConfig",
                    "tool_config",
                    "generationConfig",
                    "generation_config",
                ],
            );
        }
    };
    Ok(texts.0.join("\n"))
}

#[derive(Default)]
struct Texts(Vec<String>);

impl Texts {
    fn json_fields(&mut self, value: &Value, keys: &[&str]) {
        for key in keys {
            if let Some(value) = value.get(key).filter(|value| !value.is_null()) {
                self.0.push(value.to_string());
            }
        }
    }

    fn content(&mut self, value: &Value) {
        match value {
            Value::String(text) => self.0.push(text.clone()),
            Value::Array(items) => {
                for item in items {
                    self.content(item);
                }
            }
            Value::Object(_) => {
                // References and binary payloads are not visible text. In
                // particular, never tokenize base64, signatures or file IDs.
                if matches!(
                    value["type"].as_str(),
                    Some(
                        "image"
                            | "image_url"
                            | "input_image"
                            | "input_audio"
                            | "input_file"
                            | "file"
                            | "item_reference"
                            | "redacted_thinking"
                    )
                ) {
                    return;
                }
                for key in [
                    "text",
                    "thinking",
                    "reasoning_content",
                    "refusal",
                    "name",
                    "arguments",
                ] {
                    if let Some(text) = value[key].as_str() {
                        self.0.push(text.to_owned());
                    }
                }
                for key in ["content", "parts", "summary", "tool_calls", "function"] {
                    self.content(&value[key]);
                }
                match value["type"].as_str() {
                    Some("tool_use" | "server_tool_use" | "mcp_tool_use") => {
                        self.json_fields(value, &["input"]);
                    }
                    Some("custom_tool_call") => self.content(&value["input"]),
                    Some("function_call_output" | "custom_tool_call_output") => {
                        self.content(&value["output"]);
                    }
                    Some("document") => {
                        if value["source"]["type"] == "text" {
                            self.content(&value["source"]["data"]);
                        } else if value["source"]["type"] == "content" {
                            self.content(&value["source"]["content"]);
                        }
                    }
                    _ => {}
                }
                self.json_fields(
                    value,
                    &[
                        "functionCall",
                        "function_call",
                        "functionResponse",
                        "function_response",
                        "executableCode",
                        "executable_code",
                        "codeExecutionResult",
                        "code_execution_result",
                    ],
                );
            }
            _ => {}
        }
    }
}
