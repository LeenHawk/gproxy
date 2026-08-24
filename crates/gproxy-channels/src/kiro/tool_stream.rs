use gproxy_channel_api::{ChannelError, Frame};
use serde_json::{Value, json};

struct Call {
    id: String,
    item_id: String,
    name: String,
    arguments: String,
    index: u64,
    done: bool,
}

#[derive(Default)]
pub(super) struct Tracker {
    calls: Vec<Call>,
}

impl Tracker {
    pub(super) fn handle(
        &mut self,
        value: &Value,
        sequence: &mut u64,
    ) -> Result<Vec<Frame>, ChannelError> {
        let call_id = value
            .get("toolUseId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| ChannelError::Decode("Kiro tool event has no id".into()))?;
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let fragment = value
            .get("input")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let stop = value.get("stop").and_then(Value::as_bool).unwrap_or(false);
        let mut output = Vec::new();
        let index = match self.calls.iter().position(|call| call.id == call_id) {
            Some(index) => index,
            None => {
                if name.is_empty() {
                    return Err(ChannelError::Decode(
                        "Kiro tool event starts without a name".into(),
                    ));
                }
                let output_index = 2 + self.calls.len() as u64;
                let item_id = super::sse::id("fc", call_id);
                self.calls.push(Call {
                    id: call_id.into(),
                    item_id: item_id.clone(),
                    name: name.into(),
                    arguments: String::new(),
                    index: output_index,
                    done: false,
                });
                output.push(super::sse::frame(json!({
                    "type":"response.output_item.added",
                    "sequence_number":take(sequence),
                    "output_index":output_index,
                    "item":{
                        "type":"function_call","id":item_id,"call_id":call_id,
                        "name":name,"arguments":""
                    }
                })));
                self.calls.len() - 1
            }
        };
        if self.calls[index].name.is_empty() && !name.is_empty() {
            self.calls[index].name = name.into();
        }
        if !fragment.is_empty() {
            self.calls[index].arguments.push_str(fragment);
            output.push(super::sse::frame(json!({
                "type":"response.function_call_arguments.delta",
                "sequence_number":take(sequence),
                "output_index":self.calls[index].index,
                "item_id":self.calls[index].item_id,
                "delta":fragment
            })));
        }
        if stop {
            output.extend(self.finish_call(index, sequence));
        }
        Ok(output)
    }

    pub(super) fn is_complete(&self) -> bool {
        self.calls.iter().all(|call| call.done)
    }

    pub(super) fn items(&self) -> Vec<Value> {
        self.calls
            .iter()
            .map(|call| {
                json!({
                    "type":"function_call","id":call.item_id,"call_id":call.id,
                    "name":call.name,"arguments":call.arguments,"status":"completed"
                })
            })
            .collect()
    }

    fn finish_call(&mut self, index: usize, sequence: &mut u64) -> Vec<Frame> {
        if self.calls[index].done {
            return Vec::new();
        }
        self.calls[index].done = true;
        let call = &self.calls[index];
        vec![
            super::sse::frame(json!({
                "type":"response.function_call_arguments.done",
                "sequence_number":take(sequence),"output_index":call.index,
                "item_id":call.item_id,"arguments":call.arguments
            })),
            super::sse::frame(json!({
                "type":"response.output_item.done",
                "sequence_number":take(sequence),"output_index":call.index,
                "item":{
                    "type":"function_call","id":call.item_id,"call_id":call.id,
                    "name":call.name,"arguments":call.arguments,"status":"completed"
                }
            })),
        ]
    }
}

fn take(sequence: &mut u64) -> u64 {
    let current = *sequence;
    *sequence += 1;
    current
}
