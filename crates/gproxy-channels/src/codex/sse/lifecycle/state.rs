use gproxy_protocol::openai::common::ResponseItemLifecycleStatus;
use gproxy_protocol::openai::generate_content::responses::{
    ResponseItem, ResponseMessageItem, TypedResponseItem,
};

#[derive(Default)]
pub(super) struct ItemState {
    pub(super) item: Option<ResponseItem>,
    pub(super) added: bool,
    pub(super) input_done: bool,
    pub(super) done: bool,
    pub(super) input: String,
    pub(super) input_kind: Option<InputKind>,
    pub(super) item_id: Option<String>,
    pub(super) call_id: Option<String>,
    pub(super) name: Option<String>,
}

#[derive(Clone, Copy)]
pub(super) enum InputKind {
    Function,
    Custom,
}

impl ItemState {
    pub(super) fn note_typed_item(&mut self) {
        let Some(ResponseItem::Typed(item)) = self.item.as_ref() else {
            return;
        };
        match item.as_ref() {
            TypedResponseItem::FunctionCall {
                arguments,
                call_id,
                name,
                ..
            } => {
                self.input_kind = Some(InputKind::Function);
                arguments.clone_into(&mut self.input);
                self.call_id = Some(call_id.clone());
                self.name = Some(name.clone());
            }
            TypedResponseItem::CustomToolCall {
                input,
                call_id,
                name,
                ..
            } => {
                self.input_kind = Some(InputKind::Custom);
                input.clone_into(&mut self.input);
                self.call_id = Some(call_id.clone());
                self.name = Some(name.clone());
            }
            _ => {}
        }
    }

    pub(super) fn apply_input(&mut self) {
        let Some(ResponseItem::Typed(item)) = self.item.as_mut() else {
            return;
        };
        match item.as_mut() {
            TypedResponseItem::FunctionCall { arguments, .. } => self.input.clone_into(arguments),
            TypedResponseItem::CustomToolCall { input, .. } => self.input.clone_into(input),
            _ => {}
        }
    }

    pub(super) fn complete_item(&mut self) {
        let Some(item) = self.item.as_mut() else {
            return;
        };
        match item {
            ResponseItem::Message(ResponseMessageItem::Output(message)) => {
                message.status = ResponseItemLifecycleStatus::Completed
            }
            ResponseItem::Typed(item) => match item.as_mut() {
                TypedResponseItem::FunctionCall { status, .. }
                | TypedResponseItem::Reasoning { status, .. } => {
                    *status = Some(ResponseItemLifecycleStatus::Completed)
                }
                _ => {}
            },
            ResponseItem::Message(
                ResponseMessageItem::Input(_)
                | ResponseMessageItem::EasyInput(_)
                | ResponseMessageItem::Unknown(_),
            )
            | ResponseItem::Unknown(_) => {}
        }
    }

    pub(super) fn ensure_item(&mut self) -> Result<(), String> {
        if self.item.is_some() {
            self.apply_input();
            return Ok(());
        }
        let id = self
            .item_id
            .clone()
            .ok_or_else(|| "Codex tool stream item id is missing".to_owned())?;
        let name = self
            .name
            .clone()
            .ok_or_else(|| "Codex tool stream name is missing".to_owned())?;
        let call_id = self
            .call_id
            .clone()
            .ok_or_else(|| "Codex tool stream call_id is missing".to_owned())?;
        let item = match self.input_kind {
            Some(InputKind::Function) => TypedResponseItem::FunctionCall {
                arguments: self.input.clone(),
                call_id,
                name,
                id: Some(id),
                caller: None,
                namespace: None,
                status: Some(ResponseItemLifecycleStatus::InProgress),
                rest: Default::default(),
            },
            Some(InputKind::Custom) => TypedResponseItem::CustomToolCall {
                call_id,
                input: self.input.clone(),
                name,
                id: Some(id),
                caller: None,
                namespace: None,
                rest: Default::default(),
            },
            None => return Ok(()),
        };
        self.item = Some(ResponseItem::Typed(Box::new(item)));
        Ok(())
    }
}
