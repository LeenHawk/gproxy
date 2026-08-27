mod export;
mod import;
mod import_support;

use bytes::Bytes;
use http::Response;

use crate::{AdminError, State};

pub(super) async fn export(
    state: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    export::run(state, body).await
}

pub(super) async fn import(
    state: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    import::run(state, body).await
}
