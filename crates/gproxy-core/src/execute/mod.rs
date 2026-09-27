//! Request execution: select a credential inside the permitted set, prepare
//! and dispatch through the channel, observe every physical exchange, and
//! guarantee that settlement happens exactly once per request.

mod attempt;
mod chat_usage;
mod exchange;
mod fallback;
mod funnel;
mod native;
pub(crate) mod prepare;
mod stream;
mod websocket;

pub(crate) use attempt::run_http;
pub(crate) use exchange::{Exchange, ObservedClient};
pub(crate) use funnel::Funnel;
pub(crate) use websocket::run_websocket;

pub(crate) use native::NativeCall;
