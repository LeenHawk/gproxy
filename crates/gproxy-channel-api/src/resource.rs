use gproxy_protocol::OperationKey;
use serde_json::Value;

pub struct ResourceCtx<'a> {
    pub key: OperationKey,
    pub request_resource: Option<(&'static str, &'a str)>,
    pub request_body: &'a [u8],
    pub response_headers: &'a http::HeaderMap,
    pub response_body: &'a [u8],
}

#[derive(Debug)]
pub enum ResourceMutation {
    Save {
        kind: &'static str,
        id: String,
        summary: Value,
    },
    Delete {
        kind: &'static str,
        id: String,
    },
}
