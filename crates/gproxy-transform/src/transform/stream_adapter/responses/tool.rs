use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct ResponsesToolItemState {
    kind: Option<ResponsesToolKind>,
    pub(super) item_id: Option<String>,
    call_id: Option<String>,
    pub(super) name: Option<String>,
    output_index: Option<u32>,
    pub(super) input: String,
    pub(super) input_done: bool,
    pub(super) item_done: bool,
}

#[derive(Clone, Copy)]
pub(super) enum ResponsesToolKind {
    Function,
    Custom,
}

impl ResponsesToolItemState {
    pub(super) fn note_kind(&mut self, kind: ResponsesToolKind, index: u32) {
        self.kind.get_or_insert(kind);
        self.output_index.get_or_insert(index);
    }
    pub(super) fn note_item(&mut self, item: &Value) {
        if self.item_id.is_none() {
            self.item_id = item.get("id").and_then(Value::as_str).map(str::to_owned);
        }
        if self.call_id.is_none() {
            self.call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if self.name.is_none() {
            self.name = item.get("name").and_then(Value::as_str).map(str::to_owned);
        }
        if self.input.is_empty() {
            let field = match self.kind {
                Some(ResponsesToolKind::Function) => "arguments",
                Some(ResponsesToolKind::Custom) => "input",
                None => return,
            };
            if let Some(input) = item.get(field).and_then(Value::as_str) {
                self.input.push_str(input);
            }
        }
    }
    pub(super) fn note_event_item_id(&mut self, event: &Value) {
        if self.item_id.is_none() {
            self.item_id = event
                .get("item_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
    }
    pub(super) fn can_finish(&self) -> bool {
        self.kind.is_some()
            && self.item_id.is_some()
            && self.call_id.is_some()
            && self.name.is_some()
    }
    pub(super) fn input_done_event(&self) -> Value {
        match self.kind.expect("tool kind checked by can_finish") {
            ResponsesToolKind::Function => {
                json!({"type":"response.function_call_arguments.done","output_index":self.output_index(),"item_id":self.item_id(),"name":self.name(),"arguments":self.input})
            }
            ResponsesToolKind::Custom => {
                json!({"type":"response.custom_tool_call_input.done","output_index":self.output_index(),"item_id":self.item_id(),"input":self.input})
            }
        }
    }
    pub(super) fn item_done_event(&self) -> Value {
        json!({"type":"response.output_item.done","output_index":self.output_index(),"item":self.item("completed")})
    }
    pub(super) fn item(&self, status: &str) -> Value {
        match self.kind.expect("tool kind checked by can_finish") {
            ResponsesToolKind::Function => {
                json!({"id":self.item_id(),"type":"function_call","status":status,"call_id":self.call_id(),"name":self.name(),"arguments":self.input})
            }
            ResponsesToolKind::Custom => {
                json!({"id":self.item_id(),"type":"custom_tool_call","call_id":self.call_id(),"name":self.name(),"input":self.input})
            }
        }
    }
    fn item_id(&self) -> &str {
        self.item_id.as_deref().unwrap_or("item_0")
    }
    fn call_id(&self) -> &str {
        self.call_id.as_deref().unwrap_or("call_0")
    }
    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }
    fn output_index(&self) -> u32 {
        self.output_index.unwrap_or(0)
    }
}
