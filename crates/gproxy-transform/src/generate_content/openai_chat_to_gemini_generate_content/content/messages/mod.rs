use std::collections::BTreeMap;

use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::parts::{text_content, text_part, user_parts};

mod assistant;
mod result;

pub(crate) fn messages(
    messages: Vec<openai::ChatCompletionMessageParam>,
) -> Result<(Vec<gemini::Content>, Option<gemini::Content>), TransformError> {
    let mut state = State::default();
    for (turn, message) in messages.into_iter().enumerate() {
        state.message(message, turn)?;
    }
    Ok(state.finish())
}

struct Call {
    name: String,
    code_execution: bool,
}

#[derive(Default)]
struct State {
    contents: Vec<gemini::Content>,
    system_parts: Vec<gemini::Part>,
    system_entries: Vec<serde_json::Value>,
    calls: BTreeMap<String, Call>,
    seen_turn: bool,
}

impl State {
    fn message(
        &mut self,
        message: openai::ChatCompletionMessageParam,
        turn: usize,
    ) -> Result<(), TransformError> {
        match message {
            openai::ChatCompletionMessageParam::Developer(message) => {
                self.system(message.content, "developer", message.rest)
            }
            openai::ChatCompletionMessageParam::System(message) => {
                self.system(message.content, "system", message.rest)
            }
            openai::ChatCompletionMessageParam::User(message) => {
                self.seen_turn = true;
                let parts = user_parts(message.content)?;
                self.push(gemini::ContentRoleKnown::User, parts, message.rest);
                Ok(())
            }
            openai::ChatCompletionMessageParam::Assistant(message) => {
                self.seen_turn = true;
                self.assistant(message, turn)
            }
            openai::ChatCompletionMessageParam::Tool(message) => {
                self.seen_turn = true;
                self.tool_result(message)
            }
            openai::ChatCompletionMessageParam::Function(message) => {
                self.seen_turn = true;
                self.function_result(message)
            }
            openai::ChatCompletionMessageParam::Unknown(raw) => {
                Err(TransformError::unsupported("Chat message", raw.to_string()))
            }
        }
    }

    fn system(
        &mut self,
        content: openai::ChatTextContent,
        role: &str,
        rest: openai::Rest,
    ) -> Result<(), TransformError> {
        let text = text_content(content)?;
        if text.is_empty() {
            return Ok(());
        }
        let entry = serde_json::json!({ "role": role, "rest": rest });
        if self.seen_turn {
            let mut content_rest = openai::Rest::new();
            content_rest.insert("openai_system_message".into(), entry);
            self.push(
                gemini::ContentRoleKnown::System,
                vec![text_part(text, false, Default::default())],
                content_rest,
            );
        } else {
            self.system_parts
                .push(text_part(text, false, Default::default()));
            self.system_entries.push(entry);
        }
        Ok(())
    }

    fn push(
        &mut self,
        role: gemini::ContentRoleKnown,
        parts: Vec<gemini::Part>,
        rest: gemini::ExtraFields,
    ) {
        if !parts.is_empty() {
            if let Some(previous) = self.contents.last_mut()
                && previous.role == Some(gemini::ContentRole::Known(role.clone()))
            {
                previous.parts.extend(parts);
                previous.rest.extend(rest);
                return;
            }
            self.contents.push(gemini::Content {
                parts,
                role: Some(gemini::ContentRole::Known(role)),
                rest,
            });
        }
    }

    fn finish(mut self) -> (Vec<gemini::Content>, Option<gemini::Content>) {
        let system = (!self.system_parts.is_empty()).then(|| {
            let mut rest = gemini::ExtraFields::new();
            rest.insert(
                "openai_system_messages".into(),
                serde_json::Value::Array(self.system_entries),
            );
            gemini::Content {
                parts: self.system_parts,
                role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
                rest,
            }
        });
        (std::mem::take(&mut self.contents), system)
    }
}
