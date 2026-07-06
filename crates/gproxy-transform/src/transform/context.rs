use crate::protocol::OperationKey;

/// Per-call transform settings.
///
/// `path`/`query` carry the INBOUND request target (provider-relative, as the
/// client sent it) for transforms that need more than the body — e.g. the
/// list-models query conversion. They are filled on the request direction via
/// [`with_request`](Self::with_request); response-direction contexts leave
/// them empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformContext {
    pub source: OperationKey,
    pub target: OperationKey,
    pub path: String,
    pub query: Option<String>,
}

impl TransformContext {
    pub fn new(source: OperationKey, target: OperationKey) -> Self {
        Self {
            source,
            target,
            path: String::new(),
            query: None,
        }
    }

    /// Attach the inbound request target (request-direction contexts).
    pub fn with_request(mut self, path: &str, query: Option<&str>) -> Self {
        self.path = path.to_owned();
        self.query = query.map(str::to_owned);
        self
    }
}
