//! Explicit embedding split/batch calls over one selected upstream target.

mod gemini_openai;
mod openai_gemini;
mod openai_packing;
mod packing;
mod single;

use crate::{
    HttpBody, WireResponse,
    codec::CodecLimits,
    transform::{Converted, TransformError, embeddings::OpenAiUsageFacts},
};

pub use gemini_openai::gemini_batch_to_openai;
pub use openai_gemini::openai_to_gemini_batch;
pub use single::{gemini_single_to_openai, openai_to_gemini_single};

#[derive(Debug, Clone)]
pub struct EmbeddingBatchOptions {
    /// Bounds each HTTP call and the aggregate encoded successful output.
    pub codec: CodecLimits,
    /// Optional actual usage for each planned call, used only where the target
    /// does not return it. Empty means no supplements; otherwise length must
    /// equal the number of calls after packing by encoded bytes.
    pub usage_per_call: Vec<Option<OpenAiUsageFacts>>,
}

#[derive(Debug)]
pub struct EmbeddingBatchResult<T> {
    pub output: Converted<T>,
    pub completed_calls: usize,
}

#[derive(Debug)]
pub enum EmbeddingFailure {
    Transform(TransformError),
    Rejected(Box<WireResponse<HttpBody>>),
}

/// Failure never presents completed partial vectors as a successful batch.
/// `completed_calls` counts 2xx calls whose JSON was decoded, including one
/// whose typed data proved invalid. A transport/body failure can have an
/// unknown upstream outcome; this counter is not a retry authorization.
/// `completed_items` counts only vectors validated before the failed call.
#[derive(Debug)]
pub struct EmbeddingBatchError {
    pub completed_calls: usize,
    pub completed_items: usize,
    pub failure: EmbeddingFailure,
}

impl std::fmt::Display for EmbeddingBatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "embedding batch failed after {} completed calls and {} validated items: ",
            self.completed_calls, self.completed_items
        )?;
        match &self.failure {
            EmbeddingFailure::Transform(error) => error.fmt(f),
            EmbeddingFailure::Rejected(response) => write!(f, "upstream HTTP {}", response.status),
        }
    }
}

impl std::error::Error for EmbeddingBatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.failure {
            EmbeddingFailure::Transform(error) => Some(error),
            EmbeddingFailure::Rejected(_) => None,
        }
    }
}

impl From<TransformError> for EmbeddingBatchError {
    fn from(error: TransformError) -> Self {
        Self {
            completed_calls: 0,
            completed_items: 0,
            failure: EmbeddingFailure::Transform(error),
        }
    }
}

fn at(error: TransformError, calls: usize, items: usize) -> EmbeddingBatchError {
    EmbeddingBatchError {
        completed_calls: calls,
        completed_items: items,
        failure: EmbeddingFailure::Transform(error),
    }
}
