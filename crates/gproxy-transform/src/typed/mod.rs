//! Typed pairwise transforms for callers that already hold protocol models.

pub mod compact;
pub mod count_tokens;
pub mod embeddings;
pub mod generate_content;
pub mod images;
pub mod models;
pub mod stream;
pub mod synthesize;
pub mod videos;

/// Request-only values supplied by routing rather than the source body.
#[derive(Debug, Clone, Copy)]
pub struct RequestContext<'a> {
    pub upstream_model: &'a str,
    pub stream: bool,
}

impl<'a> RequestContext<'a> {
    pub const fn new(upstream_model: &'a str, stream: bool) -> Self {
        Self {
            upstream_model,
            stream,
        }
    }
}
