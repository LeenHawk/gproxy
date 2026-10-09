//! What the buffered, synthesized and streaming preparations of a pair share:
//! the request flags each dialect toggles, and the parts of a prepared pair
//! they read. Pair-specific mapping stays in each pair.

use std::future::Future;

use super::GenerationIdentity;
use crate::{
    transform::{Report, TransformError},
    wire::{
        DeclaredFields,
        claude::generate_content as c,
        gemini as g,
        openai::{chat as h, responses as r},
    },
};

/// A request body whose transport mode is a body flag. Gemini selects it by
/// endpoint, so its body is left unchanged.
pub(crate) trait RequestMode: DeclaredFields + Clone {
    fn set_buffered(&mut self);
    fn set_streaming(&mut self);
    fn buffered(mut self) -> Self {
        self.set_buffered();
        self
    }
    fn streaming(mut self) -> Self {
        self.set_streaming();
        self
    }
    fn emit_usage(&self) -> bool {
        true
    }
}

impl RequestMode for h::GenerateContentRequestBody {
    fn set_buffered(&mut self) {
        self.stream = Some(Some(false));
    }
    fn set_streaming(&mut self) {
        self.stream = Some(Some(true));
        let mut options = self
            .stream_options
            .take()
            .flatten()
            .unwrap_or_else(|| h::StreamOptions::builder().build())
            .into_declared();
        options.include_usage = Some(true);
        self.stream_options = Some(Some(options));
    }
    fn emit_usage(&self) -> bool {
        if self.stream.flatten() != Some(true) {
            return true;
        }
        self.stream_options
            .as_ref()
            .and_then(Option::as_ref)
            .and_then(|v| v.include_usage)
            == Some(true)
    }
}

impl RequestMode for c::GenerateContentRequestBody {
    fn set_buffered(&mut self) {
        self.stream = Some(false);
    }
    fn set_streaming(&mut self) {
        self.stream = Some(true);
    }
}

impl RequestMode for r::GenerateContentRequestBody {
    fn set_buffered(&mut self) {
        self.stream = Some(Some(false));
    }
    fn set_streaming(&mut self) {
        self.stream = Some(Some(true));
    }
}

impl RequestMode for g::GenerateContentRequestBody {
    fn set_buffered(&mut self) {}
    fn set_streaming(&mut self) {}
}

/// A pair after preparation: the client request it answers and the target
/// request it sends.
pub(crate) trait PreparedGeneration: Sized {
    type Source: RequestMode;
    type Target: RequestMode;
    fn target_request(&self) -> &Self::Target;
    fn identities(&self) -> &GenerationIdentity;
    fn report(&self) -> &Report;
    fn requests_mut(&mut self) -> (&mut Self::Source, &mut Self::Target);
}

/// Implements [`PreparedGeneration`] over a pair's own fields.
macro_rules! prepared_generation {
    ($pair:ty, $source:ty => $target:ty) => {
        impl super::prepared::PreparedGeneration for $pair {
            type Source = $source;
            type Target = $target;
            fn target_request(&self) -> &$target {
                &self.target_request
            }
            fn identities(&self) -> &super::GenerationIdentity {
                &self.identities
            }
            fn report(&self) -> &crate::transform::Report {
                &self.report
            }
            fn requests_mut(&mut self) -> (&mut $source, &mut $target) {
                (&mut self.original_request, &mut self.target_request)
            }
        }
    };
}
pub(crate) use prepared_generation;

/// Prepare a buffered upstream call whose result is later synthesized into
/// the client's native stream. The original client request, stream flag
/// included, is what the pair answers.
pub(crate) async fn for_stream_synthesis<P, F, Fut>(
    input: P::Source,
    prepare: F,
) -> Result<P, TransformError>
where
    P: PreparedGeneration,
    F: FnOnce(P::Source) -> Fut,
    Fut: Future<Output = Result<P, TransformError>>,
{
    let original = input.into_declared();
    let mut prepared = prepare(original.clone().buffered()).await?;
    let (source, target) = prepared.requests_mut();
    *source = original;
    target.set_buffered();
    Ok(prepared)
}

/// Generates a pair's two `prepare_for_stream_synthesis` entry points, which
/// differ only in whether foreign resources are materialized first. Extra
/// arguments are forwarded to the pair's own preparation.
macro_rules! stream_synthesis {
    ($pair:ty, $source:ty $(, $extra:ident: $extra_ty:ty)* $(,)?) => {
        impl $pair {
            /// Prepare a buffered upstream result for later native stream synthesis,
            /// retaining the original client stream flag and all declared controls.
            pub async fn prepare_for_stream_synthesis<S: crate::capability::StateStore>(
                input: $source,
                endpoint: super::Endpoint,
                identities: super::GenerationIdentity,
                state: &super::GenerationStateAccess<'_, S>,
                $($extra: $extra_ty,)*
            ) -> Result<Self, crate::transform::TransformError> {
                super::prepared::for_stream_synthesis(input, |buffered| {
                    Self::prepare_with_state(buffered, endpoint, identities, state $(, $extra)*)
                })
                .await
            }
            /// Prepare a buffered upstream result for later native stream synthesis,
            /// retaining the original client stream flag and all declared controls.
            pub async fn prepare_for_stream_synthesis_with_capabilities<
                S: crate::capability::StateStore,
                R: crate::capability::ResourceAccess,
            >(
                input: $source,
                endpoint: super::Endpoint,
                identities: super::GenerationIdentity,
                state: &super::GenerationStateAccess<'_, S>,
                resources: &super::GenerationResources<'_, R>,
                $($extra: $extra_ty,)*
            ) -> Result<Self, crate::transform::TransformError> {
                super::prepared::for_stream_synthesis(input, |buffered| {
                    Self::prepare_with_capabilities(
                        buffered, endpoint, identities, state, resources $(, $extra)*
                    )
                })
                .await
            }
        }
    };
}
pub(crate) use stream_synthesis;
