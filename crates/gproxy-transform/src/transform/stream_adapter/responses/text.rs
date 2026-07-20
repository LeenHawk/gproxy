use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct ResponsesTextItemState {
    pub(super) started: bool,
    done: bool,
    id: Option<String>,
    output_index: Option<u32>,
    content_index: Option<u32>,
    pub(super) text: String,
}

impl ResponsesTextItemState {
    pub(super) fn ensure(
        &mut self,
        event: &Value,
        fallback_id: &'static str,
        build: impl FnOnce(&Self) -> Value,
    ) -> Vec<Value> {
        self.note_delta_identity(event, fallback_id);
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![build(self)]
    }

    pub(super) fn push_delta(&mut self, event: &Value) {
        self.note_delta_identity(event, "item_0");
        if let Some(delta) = event.get("delta").and_then(Value::as_str) {
            self.text.push_str(delta);
        }
    }

    pub(super) fn finish(&mut self, build: impl FnOnce(&Self) -> Vec<Value>) -> Vec<Value> {
        if !self.started || self.done {
            return Vec::new();
        }
        self.done = true;
        build(self)
    }

    fn note_delta_identity(&mut self, event: &Value, fallback_id: &'static str) {
        if self.id.is_none() {
            self.id = event
                .get("item_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| Some(fallback_id.into()));
        }
        if self.output_index.is_none() {
            self.output_index = event
                .get("output_index")
                .and_then(Value::as_u64)
                .and_then(|n| n.try_into().ok())
                .or(Some(0));
        }
        if self.content_index.is_none() {
            self.content_index = event
                .get("content_index")
                .and_then(Value::as_u64)
                .and_then(|n| n.try_into().ok())
                .or(Some(0));
        }
    }

    pub(super) fn note_added(&mut self, event: &Value) {
        self.started = true;
        self.note_item_identity(event);
    }
    pub(super) fn note_item_done(&mut self, event: &Value) {
        self.done = true;
        self.note_item_identity(event);
    }
    pub(super) fn note_done_text(&mut self, event: &Value) {
        self.done = true;
        if let Some(text) = event.get("text").and_then(Value::as_str) {
            self.text = text.into();
        }
    }
    fn note_item_identity(&mut self, event: &Value) {
        if self.id.is_none() {
            self.id = event
                .get("item")
                .and_then(|item| item.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if self.output_index.is_none() {
            self.output_index = event
                .get("output_index")
                .and_then(Value::as_u64)
                .and_then(|n| n.try_into().ok());
        }
    }
    pub(super) fn id(&self) -> &str {
        self.id.as_deref().unwrap_or("item_0")
    }
    pub(super) fn output_index(&self) -> u32 {
        self.output_index.unwrap_or(0)
    }
    pub(super) fn content_index(&self) -> u32 {
        self.content_index.unwrap_or(0)
    }
}

pub(super) fn message_item_added(state: &ResponsesTextItemState) -> Value {
    json!({"type":"response.output_item.added","output_index":state.output_index(),"item":message_item(state,"in_progress")})
}
pub(super) fn reasoning_item_added(state: &ResponsesTextItemState) -> Value {
    json!({"type":"response.output_item.added","output_index":state.output_index(),"item":reasoning_item(state,"in_progress")})
}
pub(super) fn message_item(state: &ResponsesTextItemState, status: &str) -> Value {
    json!({"id":state.id(),"type":"message","status":status,"role":"assistant",
        "content":[{"type":"output_text","text":state.text,"annotations":[]}]})
}
pub(super) fn reasoning_item(state: &ResponsesTextItemState, status: &str) -> Value {
    json!({"id":state.id(),"type":"reasoning","status":status,"summary":[],
        "content":[{"type":"reasoning_text","text":state.text}]})
}
