mod parse;
mod write;

use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use http::{HeaderMap, HeaderValue};

pub(super) struct Part {
    pub name: String,
    pub file: bool,
    pub mime: Option<String>,
    pub data: Vec<u8>,
}

pub(super) fn stt(headers: &HeaderMap, body: &Bytes) -> Result<(Bytes, HeaderValue), ChannelError> {
    let parts = match parse::parts(headers, body)? {
        Some(parts) => parts,
        None => write::from_json(body)?,
    };
    write::stt(parts)
}
