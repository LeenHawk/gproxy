mod events;
mod parts;
mod state;

use crate::envelope::Converter;
pub(crate) use state::State;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::default())
}
