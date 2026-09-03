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
            openai::ChatCompletionMessageParam::Developer(message) => self.system(message.content),
            openai::ChatCompletionMessageParam::System(message) => self.system(message.content),
            openai::ChatCompletionMessageParam::User(message) => {
                self.seen_turn = true;
                let parts = user_parts(message.content)?;
                self.push(gemini::ContentRoleKnown::User, parts);
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
            openai::ChatCompletionMessageParam::Unknown(_) => Ok(()),
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        }
    }

    fn system(&mut self, content: openai::ChatTextContent) -> Result<(), TransformError> {
        let text = text_content(content)?;
        if text.is_empty() {
            return Ok(());
        }
        if self.seen_turn {
            self.push(
                gemini::ContentRoleKnown::System,
                vec![text_part(text, false)],
            );
        } else {
            self.system_parts.push(text_part(text, false));
        }
        Ok(())
    }

    fn push(&mut self, role: gemini::ContentRoleKnown, parts: Vec<gemini::Part>) {
        if !parts.is_empty() {
            if let Some(previous) = self.contents.last_mut()
                && previous.role == Some(gemini::ContentRole::Known(role.clone()))
            {
                previous.parts.extend(parts);
                return;
            }
            self.contents.push(crate::wire!(gemini::Content {
                parts,
                role: Some(gemini::ContentRole::Known(role)),
                rest: Default::default(),
            }));
        }
    }

    fn finish(mut self) -> (Vec<gemini::Content>, Option<gemini::Content>) {
        let system = (!self.system_parts.is_empty()).then(|| {
            crate::wire!(gemini::Content {
                parts: self.system_parts,
                role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
                rest: Default::default(),
            })
        });
        (std::mem::take(&mut self.contents), system)
    }
}
