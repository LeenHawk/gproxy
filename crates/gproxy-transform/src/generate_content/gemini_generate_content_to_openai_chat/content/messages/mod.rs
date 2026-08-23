use std::collections::{BTreeMap, VecDeque};

use gproxy_protocol::{gemini, openai};

use crate::TransformError;

mod model;
mod system;
mod user;

pub(crate) fn messages(
    contents: Vec<gemini::Content>,
) -> Result<Vec<openai::ChatCompletionMessageParam>, TransformError> {
    let mut state = State::default();
    let mut output = Vec::new();
    for (turn, content) in contents.into_iter().enumerate() {
        output.extend(state.content(content, turn)?);
    }
    Ok(output)
}

pub(crate) fn system_content(
    content: gemini::Content,
) -> Result<(openai::ChatTextContent, openai::Rest), TransformError> {
    system::convert(content)
}

#[derive(Default)]
struct State {
    calls: BTreeMap<String, VecDeque<String>>,
    pending_code: VecDeque<String>,
}

impl State {
    fn content(
        &mut self,
        content: gemini::Content,
        turn: usize,
    ) -> Result<Vec<openai::ChatCompletionMessageParam>, TransformError> {
        match content.role.as_ref() {
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)) => {
                self.model(content, turn)
            }
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)) => {
                let (content, rest) = system::convert(content)?;
                Ok(vec![openai::ChatCompletionMessageParam::Developer(
                    openai::ChatDeveloperMessageParam {
                        role: openai::ChatDeveloperRole::Developer,
                        content,
                        name: None,
                        rest,
                    },
                )])
            }
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Function))
            | Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::User))
            | None => self.user(content),
            Some(gemini::ContentRole::Unknown(role)) => Err(TransformError::unsupported(
                "Gemini content role",
                role.clone(),
            )),
            Some(_) => Err(TransformError::unsupported(
                "Gemini content role",
                "future role",
            )),
        }
    }
}
