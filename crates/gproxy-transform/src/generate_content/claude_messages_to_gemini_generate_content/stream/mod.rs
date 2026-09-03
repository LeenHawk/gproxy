mod chunks;
mod converter;
mod state;
mod tools;

use crate::envelope::Converter;
pub(crate) use state::State;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::default())
}
