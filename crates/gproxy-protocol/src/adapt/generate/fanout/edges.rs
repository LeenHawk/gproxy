use super::super::{
    chat_claude::ChatViaClaude, chat_responses::ChatViaResponses, claude_gemini::GeminiViaClaude,
    gemini_responses::GeminiViaResponses, identity_facts::IdentityFacts,
};
use super::*;
use crate::{
    transform::{Converted, Report},
    wire::{
        DeclaredFields,
        claude::generate_content as c,
        gemini as g,
        openai::{chat as h, responses as r},
    },
};
use serde::{Serialize, de::DeserializeOwned};
pub(super) trait Edge {
    type Request: Serialize + DeclaredFields + Clone;
    type Native: Serialize + DeserializeOwned + DeclaredFields + Clone + IdentityFacts;
    type Client: output::Client;
    type Facts;
    fn signed_bindings(&self) -> Option<&super::super::request_ids::SignedToolBindings> {
        None
    }
    fn request(&self) -> &Self::Request;
    fn identities(&self) -> &GenerationIdentity;
    fn report(&self) -> &Report;
    fn model(&self) -> &str;
    fn convert(
        &mut self,
        native: Self::Native,
        facts: Self::Facts,
    ) -> Result<Converted<Self::Client>, TransformError>;
}
macro_rules! edge {
    ($a:ty,$request:ty,$native:ty,$client:ty,$facts:ty) => {
        impl Edge for $a {
            type Request = $request;
            type Native = $native;
            type Client = $client;
            type Facts = $facts;
            fn request(&self) -> &Self::Request {
                self.target_request()
            }
            fn identities(&self) -> &GenerationIdentity {
                self.identities()
            }
            fn report(&self) -> &Report {
                self.report()
            }
            fn model(&self) -> &str {
                self.selected_model()
            }
            fn convert(
                &mut self,
                native: Self::Native,
                facts: Self::Facts,
            ) -> Result<Converted<Self::Client>, TransformError> {
                self.convert_response(native, facts)
            }
        }
    };
}
edge!(
    ChatViaClaude,
    c::GenerateContentRequestBody,
    c::GenerateContentResponseBody,
    h::GenerateContentResponseBody,
    crate::transform::generate::claude_chat::ResponseSupplement
);
edge!(
    ChatViaResponses,
    r::GenerateContentRequestBody,
    r::GenerateContentResponseBody,
    h::GenerateContentResponseBody,
    ()
);
edge!(
    GeminiViaClaude,
    c::GenerateContentRequestBody,
    c::GenerateContentResponseBody,
    g::GenerateContentResponseBody,
    crate::transform::generate::claude_gemini::ClaudeGeminiUsageFacts
);
impl Edge for GeminiViaResponses {
    type Request = r::GenerateContentRequestBody;
    type Native = r::GenerateContentResponseBody;
    type Client = g::GenerateContentResponseBody;
    type Facts = crate::transform::generate::gemini_responses::GeminiReplayContext;
    fn request(&self) -> &Self::Request {
        self.target_request()
    }
    fn identities(&self) -> &GenerationIdentity {
        self.identities()
    }
    fn report(&self) -> &Report {
        self.report()
    }
    fn model(&self) -> &str {
        self.selected_model()
    }
    fn signed_bindings(&self) -> Option<&super::super::request_ids::SignedToolBindings> {
        Some(self.signed_tool_bindings())
    }
    fn convert(
        &mut self,
        native: Self::Native,
        facts: Self::Facts,
    ) -> Result<Converted<Self::Client>, TransformError> {
        self.convert_response(native, facts)
    }
}
