use crate::common::stream::claude_to_openai::{Output, State};
use crate::envelope::Converter;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::new(Output::Responses))
}
