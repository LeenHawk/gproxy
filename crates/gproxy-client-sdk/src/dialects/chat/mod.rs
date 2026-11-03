//! OpenAI Chat is a distinct dialect because its audio history is ID-based.
use super::*;

use gproxy_protocol::openai::chat::{content as c, request as q, response as r, tools as t};

use std::collections::BTreeMap;
mod metadata;
use metadata::*;
mod render;
pub use render::{render, render_at};
mod parse;
pub(crate) use parse::finish_reason;
pub use parse::parse;
mod audio;
use audio::*;
pub(crate) use audio::{audio_reference_valid, output_format};
mod stream;
pub(crate) use stream::StreamState;

pub(crate) mod warnings;
