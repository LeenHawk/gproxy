//! Core's side of conversion: the provider's operation rules over
//! `gproxy-convert`'s default routing, the local handlers, and the Responses
//! WebSocket client bridged onto HTTP upstreams. The drivers themselves are
//! `gproxy-convert`, run over core's capabilities ([`CoreHost`]).

mod route;

pub(crate) mod local;
pub(crate) mod responses_ws;

use crate::{AttemptUpstream, ProtocolState, ResourceScope, Resources, StateScope};
use gproxy_seaorm::BatchConnectionTrait;
use std::marker::PhantomData;

pub(crate) use gproxy_convert::{
    Call, ClientRequest, Converted, ModelFacts, dispatch, stream_error, upstream_error,
};
pub use gproxy_convert::{
    Route, RoutingMappings, conversion_targets, default_route, default_route_for_model,
    generate_endpoint, is_websocket, local_model_metadata, local_supported, response_framing,
    spec_for, validate_mapping,
};
pub use route::{RouteError, resolve_route, resolve_route_for_model, route, route_for_model};

/// Core's capabilities as the drivers take them.
pub(crate) struct CoreHost<'a, C>(PhantomData<&'a C>);

impl<'a, C: BatchConnectionTrait + Send + Sync + 'static> gproxy_convert::Host for CoreHost<'a, C> {
    type Upstream = AttemptUpstream;
    type State = ProtocolState<C>;
    type StateScope = StateScope;
    type Resources = Resources<'a, C>;
    type ResourceScope = ResourceScope;

    fn spawn(task: gproxy_convert::Task) {
        crate::rt::spawn(task);
    }
}
