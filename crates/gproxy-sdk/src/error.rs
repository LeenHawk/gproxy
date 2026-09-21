//! One error type for the whole handle, with an HTTP status for hosts that
//! answer a request with it.
//!
//! Every failure a caller can act on has its own variant; the engine, store,
//! cache and transport errors stay wrapped so nothing about them is lost.
//! [`SdkError::status_code`] is the only place that maps a failure to a status,
//! so two hosts cannot disagree about what a spent budget or a dead credential
//! looks like on the wire.

pub type SdkResult<T> = Result<T, SdkError>;

#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    #[error(transparent)]
    Core(#[from] gproxy_core::CoreError),
    #[error(transparent)]
    Store(#[from] gproxy_store::StoreError),
    #[error(transparent)]
    Cache(#[from] gproxy_cache::CacheError),
    #[error(transparent)]
    Build(#[from] gproxy_core::BuildError),
    /// The database driver itself failed: connecting, or synchronizing schema.
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Channel(#[from] gproxy_channel::ChannelError),
    #[error(transparent)]
    Secret(#[from] gproxy_core::SecretError),
    #[error(transparent)]
    File(#[from] gproxy_file::Error),
    /// The request is malformed or contradicts itself: a missing field, an
    /// exposed name using a reserved prefix, a login without a codec.
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("{entity} `{id}` does not exist")]
    NotFound { entity: &'static str, id: String },
    /// The write contradicts durable state that is already there: a duplicate
    /// name, a row another writer changed first.
    #[error("conflict: {0}")]
    Conflict(String),
    /// A legitimate request this build cannot serve: a channel ability that is
    /// not implemented, a feature that was compiled out.
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
    /// No exposed model, route or provider catalog matches the requested name.
    #[error("unknown model `{0}`")]
    UnknownModel(String),
    /// The name resolved, but nothing is left to send to: every candidate is
    /// disabled, retired or blocked.
    #[error("no usable target for `{0}`")]
    NoTarget(String),
    /// The login session expired or was never started on this instance.
    #[error("login session has expired")]
    LoginExpired,
    /// An upstream refused a management-side call (a login exchange, a quota
    /// probe). The body is the upstream's own, already read.
    #[error("upstream returned {status}: {body}")]
    Upstream { status: u16, body: String },
}

impl SdkError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
    pub fn not_found(entity: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound {
            entity,
            id: id.into(),
        }
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    /// The status a host should answer with. Anything the caller cannot fix is
    /// 500; everything else says which half of the exchange has the problem.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Core(error) => core_status(error),
            Self::Invalid(_) => 400,
            Self::NotFound { .. } | Self::UnknownModel(_) => 404,
            Self::Conflict(_) => 409,
            Self::Unsupported(_) => 501,
            Self::NoTarget(_) => 503,
            Self::LoginExpired => 410,
            Self::Upstream { status, .. } => *status,
            Self::Channel(error) => channel_status(error),
            Self::Store(_)
            | Self::Cache(_)
            | Self::Build(_)
            | Self::Db(_)
            | Self::Secret(_)
            | Self::File(_) => 500,
        }
    }
}

fn core_status(error: &gproxy_core::CoreError) -> u16 {
    use gproxy_core::CoreError as E;
    match error {
        E::NotImplemented(_) => 501,
        E::InvalidTarget(_) | E::OperationMismatch { .. } | E::Route(_) => 400,
        E::NoUsableCredential | E::RefreshContended { .. } | E::CredentialDead { .. } => 503,
        E::BudgetExhausted { .. } => 429,
        E::Forbidden(_) => 403,
        // The continuation lives in another process; this one is the wrong
        // address for it, which is exactly what 421 says.
        E::ContinuationElsewhere { .. } => 421,
        // No status reaches a cancelled client; 499 records who gave up.
        E::Cancelled => 499,
        E::DeadlineExceeded => 504,
        E::Transform(error) => transform_status(error),
        E::Channel(error) => channel_status(error),
        E::Rewrite(_)
        | E::File(_)
        | E::Secret(_)
        | E::Limits(_)
        | E::Assembly(_)
        | E::Cache(_)
        | E::Store(_) => 500,
    }
}

fn transform_status(error: &gproxy_protocol::transform::TransformError) -> u16 {
    use gproxy_protocol::transform::TransformErrorKind as K;
    match error.kind() {
        K::InvalidInput | K::MissingMetadata => 400,
        K::Unsupported => 501,
        K::Conflict => 409,
        K::MissingState => 404,
        K::Limit => 413,
        K::InvalidResult | K::Host => 500,
    }
}

fn channel_status(error: &gproxy_channel::ChannelError) -> u16 {
    use gproxy_channel::ChannelError as E;
    match error {
        E::UnsupportedService | E::UnsupportedOperation(_) | E::WrongTransport(_) => 501,
        E::InvalidConfig(_) => 400,
        E::InvalidCredential | E::RefreshRejected(_) => 401,
        E::UpstreamResponse { status, .. } => status.as_u16(),
        E::ContinuationElsewhere { .. } => 421,
        E::Host(_) => 500,
        E::InvalidResponse(_) | E::Transport(_) => 502,
    }
}
