use gproxy_channel_api::{ChannelError, DriverInput, OperationDriver, OperationStep, PrepareCtx};
use serde_json::Value;

use crate::claudeweb::stream::{Codec, SessionState};

pub(super) struct NewTurn {
    requests: super::super::prepare::Requests,
    web: super::super::request::WebRequest,
    conversation: String,
    files: Vec<Value>,
    upload: usize,
    stage: Stage,
    created: bool,
}

enum Stage {
    Start,
    Upload,
    Create,
    Settings,
}

impl NewTurn {
    pub(super) fn new(ctx: PrepareCtx<'_>, request: &Value) -> Result<Self, ChannelError> {
        let requests = super::super::prepare::Requests::new(ctx.secret, ctx.provider_settings)?;
        let prompt = ctx
            .provider_settings
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let timezone = ctx
            .provider_settings
            .get("timezone")
            .and_then(Value::as_str)
            .unwrap_or("UTC");
        let web = super::super::request::build(request, ctx.upstream_model, prompt, timezone)?;
        Ok(Self {
            requests,
            web,
            conversation: super::super::id::uuid(),
            files: Vec::new(),
            upload: 0,
            stage: Stage::Start,
            created: false,
        })
    }

    fn upload(&self) -> Result<OperationStep, ChannelError> {
        Ok(OperationStep::Call {
            label: "upload",
            request: Box::new(
                self.requests
                    .upload(&self.conversation, &self.web.uploads[self.upload])?,
            ),
        })
    }

    fn create(&self) -> Result<OperationStep, ChannelError> {
        Ok(OperationStep::Call {
            label: "conversation_create",
            request: Box::new(self.requests.create(&self.conversation)?),
        })
    }
}

impl OperationDriver for NewTurn {
    fn next(&mut self, input: Option<DriverInput>) -> Result<OperationStep, ChannelError> {
        match self.stage {
            Stage::Start if !self.web.uploads.is_empty() => {
                self.stage = Stage::Upload;
                self.upload()
            }
            Stage::Start => {
                self.stage = Stage::Create;
                self.create()
            }
            Stage::Upload => {
                let response = super::response(input)?;
                let value: Value = serde_json::from_slice(&response.body)
                    .map_err(|error| ChannelError::Prepare(format!("upload JSON: {error}")))?;
                let id = value
                    .get("file_uuid")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ChannelError::Prepare("upload file_uuid missing".into()))?;
                self.files.push(Value::String(id.into()));
                self.upload += 1;
                if self.upload < self.web.uploads.len() {
                    self.upload()
                } else {
                    self.stage = Stage::Create;
                    self.create()
                }
            }
            Stage::Create => {
                super::response(input)?;
                self.created = true;
                self.stage = Stage::Settings;
                Ok(OperationStep::Call {
                    label: "conversation_settings",
                    request: Box::new(
                        self.requests
                            .settings(&self.conversation, self.web.extended)?,
                    ),
                })
            }
            Stage::Settings => {
                super::response(input)?;
                self.web.body["files"] = Value::Array(std::mem::take(&mut self.files));
                let state = SessionState {
                    conversation: self.conversation.clone(),
                    model: self.web.model.clone(),
                    message_id: super::super::id::fresh("msg"),
                    input_tokens: self.web.input_tokens,
                };
                Ok(OperationStep::Final {
                    label: "completion",
                    request: Box::new(
                        self.requests
                            .completion(&self.conversation, &self.web.body)?,
                    ),
                    stream: Box::new(Codec::new(state, false)),
                    cleanup: Box::new(self.requests.cleanup(&self.conversation)?),
                    ttl_secs: 10 * 60,
                })
            }
        }
    }

    fn abort(&mut self) -> Option<gproxy_channel_api::PreparedRequest> {
        self.created
            .then(|| self.requests.cleanup(&self.conversation).ok())
            .flatten()
    }
}
