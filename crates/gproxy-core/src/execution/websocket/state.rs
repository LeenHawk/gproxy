use bytes::Bytes;
use gproxy_channel_api::WsFrame;

use super::ResponsesBridge;
use crate::host::Host;
use crate::usage::Ended;

pub(super) struct ActiveResponse {
    pub(super) facts: crate::funnel::FunnelCtx,
    pub(super) response_id: Option<String>,
    pub(super) pending_injections: u32,
    pub(super) pending_steers: u32,
    pub(super) terminal: Option<Ended>,
    pub(super) responses: Vec<Bytes>,
    pub(super) output_chars: u64,
}

impl ActiveResponse {
    pub(super) fn new(facts: crate::funnel::FunnelCtx) -> Self {
        Self {
            facts,
            response_id: None,
            pending_injections: 0,
            pending_steers: 0,
            terminal: None,
            responses: Vec::new(),
            output_chars: 0,
        }
    }
}

impl<H: Host> ResponsesBridge<H> {
    pub(super) fn queue_error(&mut self, status: u16, message: &str) {
        self.queued.push_back(WsFrame::Text(
            serde_json::json!({
                "type":"error","status":status,"status_code":status,
                "error":{"type":"gproxy_error","message":message}
            })
            .to_string(),
        ));
    }

    pub(super) fn queue_inject_failed(
        &mut self,
        response_id: &str,
        input: Vec<gproxy_protocol::openai::ResponseItem>,
        code: &str,
    ) {
        self.queued.push_back(WsFrame::Text(
            serde_json::json!({
                "type":"response.inject.failed","response_id":response_id,"input":input,
                "error":{"code":code,"message":"response is not active"}
            })
            .to_string(),
        ));
    }

    pub(super) fn queue_steer_failed(
        &mut self,
        request: gproxy_protocol::openai::ResponseSteerWebSocketRequest,
        code: &str,
    ) {
        self.queued.push_back(WsFrame::Text(
            serde_json::json!({
                "type":"response.steer.failed",
                "steer":{
                    "previous_response_id":request.previous_response_id,
                    "input":request.input
                },
                "error":{
                    "type":"invalid_request_error",
                    "code":code,
                    "message":"steering is unavailable for this response"
                }
            })
            .to_string(),
        ));
    }
}
