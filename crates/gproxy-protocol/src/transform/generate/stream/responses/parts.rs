use super::*;

pub(super) struct Part {
    pub value: s::OutputContentPart,
    pub summary: bool,
    pub text_done: bool,
    pub done: bool,
    pub logs: Vec<s::StreamLogprob>,
    pub annotation_index: usize,
    pub incomplete: bool,
    pub seeded: bool,
}

impl Part {
    pub fn new(value: s::OutputContentPart, summary: bool) -> Self {
        let annotation_index = match &value {
            s::OutputContentPart::Text(v) => v.annotations.len(),
            _ => 0,
        };
        Self {
            annotation_index,
            incomplete: false,
            seeded: false,
            logs: match &value {
                s::OutputContentPart::Text(value) => stream_logs(&value.logprobs),
                _ => Vec::new(),
            },
            value,
            summary,
            text_done: false,
            done: false,
        }
    }
    pub fn text(&self) -> &str {
        match &self.value {
            s::OutputContentPart::Text(v) => &v.text,
            s::OutputContentPart::Refusal(v) => &v.refusal,
            s::OutputContentPart::Reasoning(v) => &v.text,
        }
    }
    pub fn text_mut(&mut self) -> &mut String {
        match &mut self.value {
            s::OutputContentPart::Text(v) => &mut v.text,
            s::OutputContentPart::Refusal(v) => &mut v.refusal,
            s::OutputContentPart::Reasoning(v) => &mut v.text,
        }
    }
    pub fn check_kind(&self, kind: u8) -> Result<(), TransformError> {
        let actual = match &self.value {
            s::OutputContentPart::Text(_) => 0,
            s::OutputContentPart::Refusal(_) => 1,
            s::OutputContentPart::Reasoning(_) => {
                if self.summary {
                    3
                } else {
                    2
                }
            }
        };
        if self.done || self.text_done || actual != kind {
            Err(invalid("content event phase/type mismatch"))
        } else {
            Ok(())
        }
    }
    pub fn finish_text(
        &mut self,
        text: &str,
        logs: &[s::StreamLogprob],
    ) -> Result<(), TransformError> {
        if self.text() != text || (!self.logs.is_empty() && !stream_logs_match(&self.logs, logs)) {
            return Err(invalid("text done contradicts accumulated delta"));
        }
        self.seeded = false;
        self.logs = logs.to_vec();
        self.text_done = true;
        Ok(())
    }
    pub fn finish(&mut self, value: s::OutputContentPart) -> Result<(), TransformError> {
        if self.done
            || !self.text_done
            || std::mem::discriminant(&self.value) != std::mem::discriminant(&value)
        {
            return Err(invalid("part done phase/type mismatch"));
        }
        let text = match &value {
            s::OutputContentPart::Text(v) => &v.text,
            s::OutputContentPart::Refusal(v) => &v.refusal,
            s::OutputContentPart::Reasoning(v) => &v.text,
        };
        if self.text() != text {
            return Err(invalid("part done contradicts text"));
        }
        if let (s::OutputContentPart::Text(a), s::OutputContentPart::Text(b)) =
            (&self.value, &value)
            && (!b.annotations.starts_with(&a.annotations)
                || (!a.logprobs.is_empty() && !b.logprobs.starts_with(&a.logprobs))
                || (!self.logs.is_empty() && !logs_match(&self.logs, &b.logprobs)))
        {
            return Err(invalid("part done contradicts annotations/logprobs"));
        }
        self.value = value;
        self.done = true;
        Ok(())
    }
    pub fn check_final(
        &self,
        item: &r::ResponseOutputItem,
        summary: bool,
        index: i64,
    ) -> Result<(), TransformError> {
        if !self.done && !self.seeded {
            return Err(invalid("content part missing done"));
        }
        let index = usize::try_from(index).map_err(|_| invalid("negative part index"))?;
        let equal = match (item, &self.value, summary) {
            (r::ResponseOutputItem::Message(v), s::OutputContentPart::Text(p), false) => {
                v.content.get(index) == Some(&i::OutputContent::Text(p.clone()))
            }
            (r::ResponseOutputItem::Message(v), s::OutputContentPart::Refusal(p), false) => {
                v.content.get(index) == Some(&i::OutputContent::Refusal(p.clone()))
            }
            (r::ResponseOutputItem::Reasoning(v), s::OutputContentPart::Reasoning(p), true) => {
                v.summary.get(index).is_some_and(|v| v.text == p.text)
            }
            (r::ResponseOutputItem::Reasoning(v), s::OutputContentPart::Reasoning(p), false) => v
                .content
                .as_ref()
                .and_then(|v| v.get(index))
                .is_some_and(|v| v.text == p.text),
            _ => false,
        };
        if equal {
            Ok(())
        } else {
            Err(invalid("final item contradicts completed content part"))
        }
    }
}

pub(super) fn stream_logs(logs: &[i::OutputLogprob]) -> Vec<s::StreamLogprob> {
    logs.iter()
        .map(|v| s::StreamLogprob {
            token: v.token.clone(),
            logprob: v.logprob.clone(),
            top_logprobs: Some(
                v.top_logprobs
                    .iter()
                    .map(|v| s::StreamTopLogprob {
                        token: Some(v.token.clone()),
                        logprob: Some(v.logprob.clone()),
                        rest: Default::default(),
                    })
                    .collect(),
            ),
            rest: Default::default(),
        })
        .collect()
}

fn logs_match(stream: &[s::StreamLogprob], final_logs: &[i::OutputLogprob]) -> bool {
    stream.len() == final_logs.len()
        && stream.iter().zip(final_logs).all(|(a, b)| {
            a.token == b.token
                && a.logprob == b.logprob
                && a.top_logprobs.as_ref().is_none_or(|top| {
                    top.len() == b.top_logprobs.len()
                        && top.iter().zip(&b.top_logprobs).all(|(a, b)| {
                            a.token.as_ref().is_none_or(|v| v == &b.token)
                                && a.logprob.as_ref().is_none_or(|v| v == &b.logprob)
                        })
                })
        })
}

fn stream_logs_match(known: &[s::StreamLogprob], actual: &[s::StreamLogprob]) -> bool {
    known.len() == actual.len()
        && known.iter().zip(actual).all(|(a, b)| {
            a.token == b.token
                && a.logprob == b.logprob
                && match (&a.top_logprobs, &b.top_logprobs) {
                    (Some(top), Some(other)) => {
                        top.len() == other.len()
                            && top.iter().zip(other).all(|(a, b)| {
                                a.token
                                    .as_ref()
                                    .zip(b.token.as_ref())
                                    .is_none_or(|(a, b)| a == b)
                                    && a.logprob
                                        .as_ref()
                                        .zip(b.logprob.as_ref())
                                        .is_none_or(|(a, b)| a == b)
                            })
                    }
                    _ => true,
                }
        })
}
