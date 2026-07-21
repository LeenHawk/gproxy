//! Tasklet consumer-web channel over its asynchronous Agent API.

mod auth;
mod bridge;
pub(crate) mod mcp;
mod models;
mod request;
mod response;
mod routing;
mod session;
mod stream;
#[cfg(test)]
mod tests;

use bytes::Bytes;

use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest};
use crate::protocol::Provider;

pub struct TaskletChannel;

impl TaskletChannel {
    pub const ID: &'static str = "tasklet";
}

#[async_trait::async_trait]
impl Channel for TaskletChannel {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        routing::table()
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        session::prepare(ctx)
    }

    fn bundled_models(&self) -> Option<Bytes> {
        Some(models::catalog())
    }
}
