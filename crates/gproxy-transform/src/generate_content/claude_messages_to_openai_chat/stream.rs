use crate::common::stream::openai_to_claude::{Input, State};
use crate::envelope::Converter;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::new(Input::Chat))
}
