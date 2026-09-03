use std::collections::{BTreeMap, VecDeque};

use gproxy_protocol::{gemini, openai};

use crate::TransformError;

mod media;
mod messages;
mod native;
mod parts;
mod wire;

pub(in crate::generate_content) struct ContentConverter {
    next_call: u32,
    next_message: u32,
    next_reasoning: u32,
    calls_by_name: BTreeMap<String, VecDeque<String>>,
    code_calls: VecDeque<String>,
}

impl ContentConverter {
    pub(in crate::generate_content) fn new() -> Self {
        Self {
            next_call: 0,
            next_message: 0,
            next_reasoning: 0,
            calls_by_name: BTreeMap::new(),
            code_calls: VecDeque::new(),
        }
    }

    pub(in crate::generate_content) fn request(
        &mut self,
        contents: Vec<gemini::Content>,
    ) -> Result<Vec<openai::ResponseItem>, TransformError> {
        let mut output = Vec::new();
        for content in contents {
            let response = matches!(
                content.role,
                Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model))
            );
            output.extend(self.content(content, response)?);
        }
        Ok(output)
    }

    pub(in crate::generate_content) fn response(
        &mut self,
        content: gemini::Content,
    ) -> Result<Vec<openai::ResponseItem>, TransformError> {
        let mut output = Vec::new();
        let mut buffered = Vec::new();
        for part in content.parts {
            let signature = matches!(part.data, Some(gemini::PartData::FunctionCall { .. }))
                .then(|| part.thought_signature.clone())
                .flatten();
            if let Some(signature) = signature {
                output.extend(self.content(
                    gemini::Content {
                        parts: std::mem::take(&mut buffered),
                        role: content.role.clone(),
                        rest: content.rest.clone(),
                    },
                    true,
                )?);
                output.push(self.reasoning(None, Some(signature), Default::default()));
                output.extend(self.content(
                    gemini::Content {
                        parts: vec![part],
                        role: content.role.clone(),
                        rest: content.rest.clone(),
                    },
                    true,
                )?);
            } else {
                buffered.push(part);
            }
        }
        output.extend(self.content(
            gemini::Content {
                parts: buffered,
                role: content.role,
                rest: content.rest,
            },
            true,
        )?);
        Ok(output)
    }

    fn content(
        &mut self,
        content: gemini::Content,
        response: bool,
    ) -> Result<Vec<openai::ResponseItem>, TransformError> {
        let mut output = Vec::new();
        let mut message_parts = Vec::new();
        for part in content.parts {
            if let Some(item) = self.part(part, response, &mut message_parts)? {
                messages::flush(
                    &mut output,
                    &mut message_parts,
                    content.role.as_ref(),
                    response,
                    content.rest.clone(),
                    &mut self.next_message,
                );
                output.push(item);
            }
        }
        messages::flush(
            &mut output,
            &mut message_parts,
            content.role.as_ref(),
            response,
            content.rest,
            &mut self.next_message,
        );
        Ok(output)
    }

    fn allocate_call(&mut self, source: Option<String>) -> String {
        let id = super::ids::call_id(source, self.next_call);
        self.next_call = self.next_call.saturating_add(1);
        id
    }

    fn allocate_named_call(&mut self, source: Option<String>, name: &str) -> String {
        let id = source.unwrap_or_else(|| format!("call_{name}"));
        self.next_call = self.next_call.saturating_add(1);
        id
    }
}
