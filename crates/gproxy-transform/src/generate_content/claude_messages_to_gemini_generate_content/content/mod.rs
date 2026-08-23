mod functions;
mod media;
mod native;
mod request;
mod response;
mod validate;

pub(crate) use request::{request_messages, system};
pub(crate) use response::{response_block, response_content};

pub(super) fn text_part(
    text: String,
    rest: serde_json::Map<String, serde_json::Value>,
) -> gproxy_protocol::gemini::Part {
    gproxy_protocol::gemini::Part {
        thought: None,
        thought_signature: None,
        part_metadata: None,
        media_resolution: None,
        data: Some(gproxy_protocol::gemini::PartData::Text {
            text,
            rest: Default::default(),
        }),
        metadata: None,
        rest,
    }
}

pub(super) fn model_content(
    parts: Vec<gproxy_protocol::gemini::Part>,
) -> gproxy_protocol::gemini::Content {
    gproxy_protocol::gemini::Content {
        parts,
        role: Some(gproxy_protocol::gemini::ContentRole::Known(
            gproxy_protocol::gemini::ContentRoleKnown::Model,
        )),
        rest: Default::default(),
    }
}
