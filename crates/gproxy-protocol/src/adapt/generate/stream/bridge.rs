//! Sealed pair bindings over concrete native wire types; no shared content model.

use super::super::{GenerationProgress, GenerationStateAccess};
use super::event::{Collected, NativeEvent};
use crate::{
    capability::StateStore,
    transform::{
        Converted, Report, TransformError,
        generate::{
            chat_responses::stream as hr, claude_chat::stream as ch, claude_gemini::stream as cg,
            claude_responses::stream as cr, gemini_chat::stream as gh,
            gemini_responses::stream as gr,
        },
        identity::IdentityFlow,
    },
    wire::{
        DeclaredFields,
        claude::{generate_content as c, stream as cs},
        gemini as g,
        openai::{
            chat::{self as h, stream as hs},
            responses::{self as r, stream as rs},
        },
    },
};
use serde::Serialize;

pub(crate) mod sealed {
    pub trait Bridge {}
}

pub struct BridgeEnd<E> {
    pub chunks: Vec<E>,
    pub identities: IdentityFlow,
    pub report: Report,
    pub signed_tool_bindings: gr::SignedToolBindings,
}

pub trait StreamBridge: sealed::Bridge + Sized {
    type NativeEvent: NativeEvent;
    type ClientEvent: NativeEvent;
    type ClientRequest: Serialize + Clone + DeclaredFields;
    type NativeRequest: Serialize + Clone + DeclaredFields;
    fn identities(&self) -> &IdentityFlow;
    fn push(
        &mut self,
        event: Self::NativeEvent,
    ) -> Result<Converted<Vec<Self::ClientEvent>>, TransformError>;
    fn push_done(&mut self) -> Result<(), TransformError> {
        Err(TransformError::invalid_result(
            "generation.stream",
            "unexpected native DONE",
        ))
    }
    fn finish(self) -> Result<BridgeEnd<Self::ClientEvent>, TransformError>;
    fn signed_tool_bindings(&self) -> Option<&gr::SignedToolBindings> {
        None
    }
    fn save_final<S: StateStore>(
        state: &GenerationStateAccess<'_, S>,
        native: &Collected<<Self::NativeEvent as NativeEvent>::Full>,
        client: &<Self::ClientEvent as NativeEvent>::Full,
        flow: &IdentityFlow,
        bindings: &gr::SignedToolBindings,
        progress: &mut GenerationProgress<Collected<<Self::NativeEvent as NativeEvent>::Full>>,
    ) -> impl std::future::Future<Output = Result<(), TransformError>>;
}

macro_rules! bridge {
    ($ty:ty, $native:ty, $client:ty, $native_request:ty, $client_request:ty $(, done $done:ident)? $(, signed $signed:ident)?) => {
        impl sealed::Bridge for $ty {}
        impl StreamBridge for $ty {
            type NativeEvent = $native;
            type ClientEvent = $client;
            type NativeRequest = $native_request;
            type ClientRequest = $client_request;
            fn identities(&self) -> &IdentityFlow { <$ty>::identities(self) }
            fn push(&mut self, event: Self::NativeEvent) -> Result<Converted<Vec<Self::ClientEvent>>, TransformError> { <$ty>::push(self, event) }
            $(fn push_done(&mut self) -> Result<(), TransformError> { <$ty>::$done(self) })?
            fn finish(self) -> Result<BridgeEnd<Self::ClientEvent>, TransformError> {
                let end = <$ty>::finish(self)?;
                let bindings = bridge!(@bindings end $(,$signed)?);
                Ok(BridgeEnd { chunks: end.chunks, identities: end.identities, report: end.report, signed_tool_bindings: bindings })
            }
            $(bridge!(@getter $ty, $signed);)?
            async fn save_final<S: StateStore>(
                state: &GenerationStateAccess<'_, S>, native: &Collected<<Self::NativeEvent as NativeEvent>::Full>,
                client: &<Self::ClientEvent as NativeEvent>::Full, flow: &IdentityFlow,
                bindings: &gr::SignedToolBindings,
                progress: &mut GenerationProgress<Collected<<Self::NativeEvent as NativeEvent>::Full>>,
            ) -> Result<(), TransformError> {
                let proof = super::super::request_ids::SignedToolBindings::from_stream(bindings);
                state.save_pair_with_bound_ids(native, client, flow, &proof, progress).await
            }
        }
    };
    (@bindings $end:ident) => { gr::SignedToolBindings::default() };
    (@bindings $end:ident, $signed:ident) => { $end.signed_tool_bindings };
    (@getter $ty:ty, with_getter) => {
        fn signed_tool_bindings(&self) -> Option<&gr::SignedToolBindings> { Some(<$ty>::signed_tool_bindings(self)) }
    };
    (@getter $ty:ty, no_getter) => {};
}
bridge!(
    ch::ClaudeToChatStream,
    cs::StreamEvent,
    hs::ChatCompletionChunk,
    c::GenerateContentRequestBody,
    h::GenerateContentRequestBody
);
bridge!(ch::ChatToClaudeStream, hs::ChatCompletionChunk, cs::StreamEvent, h::GenerateContentRequestBody, c::GenerateContentRequestBody, done push_done);
bridge!(
    cg::ClaudeToGeminiStream,
    cs::StreamEvent,
    g::GenerateContentResponseBody,
    c::GenerateContentRequestBody,
    g::GenerateContentRequestBody
);
bridge!(
    cg::GeminiToClaudeStream,
    g::GenerateContentResponseBody,
    cs::StreamEvent,
    g::GenerateContentRequestBody,
    c::GenerateContentRequestBody
);
bridge!(
    cr::ClaudeToResponsesStream,
    cs::StreamEvent,
    rs::StreamEvent,
    c::GenerateContentRequestBody,
    r::GenerateContentRequestBody
);
bridge!(
    cr::ResponsesToClaudeStream,
    rs::StreamEvent,
    cs::StreamEvent,
    r::GenerateContentRequestBody,
    c::GenerateContentRequestBody
);
bridge!(hr::ChatToResponsesStream, hs::ChatCompletionChunk, rs::StreamEvent, h::GenerateContentRequestBody, r::GenerateContentRequestBody, done push_done);
bridge!(
    hr::ResponsesToChatStream,
    rs::StreamEvent,
    hs::ChatCompletionChunk,
    r::GenerateContentRequestBody,
    h::GenerateContentRequestBody
);
bridge!(gh::ChatToGeminiStream, hs::ChatCompletionChunk, g::GenerateContentResponseBody, h::GenerateContentRequestBody, g::GenerateContentRequestBody, done push_done);
bridge!(
    gh::GeminiToChatStream,
    g::GenerateContentResponseBody,
    hs::ChatCompletionChunk,
    g::GenerateContentRequestBody,
    h::GenerateContentRequestBody
);
bridge!(gr::GeminiToResponsesStream, g::GenerateContentResponseBody, rs::StreamEvent, g::GenerateContentRequestBody, r::GenerateContentRequestBody, signed no_getter);
bridge!(gr::ResponsesToGeminiStream, rs::StreamEvent, g::GenerateContentResponseBody, r::GenerateContentRequestBody, g::GenerateContentRequestBody, signed with_getter);
