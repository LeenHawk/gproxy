use super::*;

use gproxy_protocol::openai::responses::{generate as q, input as i, response as r, tools as t};
mod metadata;
use metadata::*;
mod render;
pub(crate) use render::is_output_image;
pub use render::render;
mod parse;
pub(crate) use parse::{
    error_finish_reason, finish_metadata, parse_item, parse_usage, response_metadata,
};
pub use parse::{parse, parse_tool_result};
mod tools;
use tools::*;

mod stream;
pub(crate) use stream::StreamState;

pub(crate) mod warnings;
