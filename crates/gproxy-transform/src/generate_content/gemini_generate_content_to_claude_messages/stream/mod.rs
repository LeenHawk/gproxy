mod events;
mod parts;
mod state;

use crate::envelope::Converter;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(state::State::default())
}
