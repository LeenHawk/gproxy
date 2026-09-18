//! Request execution: select a credential inside the permitted set, prepare
//! and dispatch through the channel, observe every physical exchange, and
//! guarantee that settlement happens exactly once per request.

mod attempt;
mod exchange;
mod funnel;
mod prepare;
mod stream;

pub(crate) use attempt::run_http;
pub(crate) use exchange::{Exchange, ObservedClient};
pub(crate) use funnel::Funnel;
