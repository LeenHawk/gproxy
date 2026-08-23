mod chunks;
mod converter;
mod state;
mod tools;

use crate::envelope::Converter;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(state::State::default())
}
