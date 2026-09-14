use super::{
    common::{invalid, limit},
    identity,
    response_items::{Kind, Part},
    responses_to_gemini::ResponsesToGeminiStream,
};
use crate::{transform::TransformError, wire::gemini as g};
use std::collections::BTreeMap;
impl ResponsesToGeminiStream {
    pub(super) fn flush(
        &mut self,
        out: &mut Vec<g::GenerateContentResponseBody>,
    ) -> Result<(), TransformError> {
        while let Some(mut item) = self.items.remove(&self.cursor) {
            let advance = match &mut item.kind {
                Kind::Excluded => item.done,
                Kind::Image { value, emitted } => {
                    if item.done && !*emitted
                        && value.status != crate::wire::openai::responses::input::ImageGenerationStatus::Completed {
                        // A nonterminal image is not an image chunk. Hold its
                        // disposition until the actual response terminal; the
                        // canonical converter distinguishes incomplete from a
                        // malformed completed response without fabricating bytes.
                        if self.terminal {
                            self.release(item.held);
                            item.held = 0;
                            *emitted = true;
                        }
                    } else if item.done && !*emitted {
                        let model = self
                            .model
                            .as_deref()
                            .ok_or_else(|| invalid("missing native model"))?;
                        let part = super::super::images::restore(
                            (**value).clone(),
                            model,
                            &mut self.restoration,
                            self.limits.max_bytes as u64,
                        )?;
                        if self.jpeg_only {
                            let mime = part
                                .inline_data
                                .as_ref()
                                .map(|b| b.mime_type.as_str())
                                .or_else(|| {
                                    part.file_data.as_ref().and_then(|f| f.mime_type.as_deref())
                                });
                            if mime != Some("image/jpeg") {
                                return Err(invalid(
                                    "actual image MIME differs from requested JPEG output",
                                ));
                            }
                        }
                        self.emit_part(part, out)?;
                        self.release(item.held);
                        item.held = 0;
                        *emitted = true;
                    }
                    *emitted
                }
                Kind::Message { parts, next } => {
                    self.flush_parts(parts, next, false, &mut item.held, out)?;
                    item.done && *next == parts.len() as i64
                }
                Kind::Reasoning {
                    parts,
                    next,
                    has_content,
                    signed,
                    final_item,
                    projected,
                } => {
                    if *signed || !*has_content {
                        if let Some(reasoning) = final_item.take() {
                            let part = if *signed {
                                let model = self
                                    .model
                                    .as_deref()
                                    .ok_or_else(|| invalid("missing native model"))?;
                                let part = super::super::identity::reasoning(
                                    reasoning,
                                    model,
                                    &mut self.restoration,
                                )?;
                                if part.function_call.is_some() {
                                    return Err(invalid(
                                        "signed reasoning also contains a function",
                                    ));
                                }
                                Some(part)
                            } else {
                                let text = reasoning
                                    .content
                                    .map(|v| {
                                        v.into_iter().map(|p| p.text).collect::<Vec<_>>().join("")
                                    })
                                    .unwrap_or_else(|| {
                                        reasoning
                                            .summary
                                            .into_iter()
                                            .map(|p| p.text)
                                            .collect::<Vec<_>>()
                                            .join("")
                                    });
                                (!text.is_empty())
                                    .then(|| g::Part::builder().thought(true).text(text).build())
                            };
                            if let Some(part) = part {
                                self.emit_part(part, out)?;
                            }
                            self.release(item.held);
                            item.held = 0;
                            *projected = true;
                        }
                        item.done && *projected
                    } else {
                        self.flush_parts(parts, next, true, &mut item.held, out)?;
                        item.done && *next == parts.len() as i64
                    }
                }
                Kind::Function {
                    value,
                    ready,
                    emitted,
                } => {
                    if *ready && !*emitted {
                        let model = self
                            .model
                            .as_deref()
                            .ok_or_else(|| invalid("missing native model"))?;
                        let mut part = super::super::identity::function(
                            (**value).clone(),
                            model,
                            &mut self.restoration,
                        )?;
                        identity::call(
                            &mut part,
                            value,
                            self.cursor,
                            (&mut self.flow, &self.policy),
                            (&self.reserved, &mut self.used, &mut self.signed),
                        )?;
                        self.emit_part(part, out)?;
                        self.release(item.held);
                        item.held = 0;
                        *emitted = true;
                    }
                    *emitted
                }
            };
            if advance {
                self.release(item.held);
                item.held = 0;
            }
            self.items.insert(self.cursor, item);
            if !advance {
                break;
            }
            self.cursor = self.cursor.checked_add(1).ok_or_else(limit)?;
        }
        Ok(())
    }
    fn flush_parts(
        &mut self,
        parts: &mut BTreeMap<i64, Part>,
        next: &mut i64,
        thought: bool,
        held: &mut usize,
        out: &mut Vec<g::GenerateContentResponseBody>,
    ) -> Result<(), TransformError> {
        while let Some(part) = parts.get_mut(next) {
            if !part.emitted || !part.pending.is_empty() {
                let text = std::mem::take(&mut part.pending);
                let bytes = text.len();
                // The buffered pair omits unsigned empty reasoning. Empty
                // message text remains a real native part; signed thinking is
                // projected atomically through its original native record.
                if !thought || !text.is_empty() {
                    let mut value = g::Part::builder().text(text).build();
                    if thought {
                        value.thought = Some(true);
                    }
                    self.emit_part(value, out)?;
                }
                self.release(bytes);
                *held -= bytes;
                part.emitted = true;
            }
            if !part.done {
                break;
            }
            *next = next.checked_add(1).ok_or_else(limit)?;
        }
        Ok(())
    }
}
