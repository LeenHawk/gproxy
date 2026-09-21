use std::fmt;

/// Errors from flow identity allocation and scoped identity persistence.
#[derive(Debug)]
pub enum IdentityError {
    InvalidIdentity(String),
    ResourceIdRequired,
    NoGeneratedPrefix,
    DuplicateSourceIdentity {
        role: IdentityRoleName,
        dialect: String,
        source_id: String,
        first_index: u64,
        duplicate_index: u64,
    },
    AmbiguousSourceIdentity(String),
    Collision(String),
    MissingState,
    ExpiredState,
    Conflict,
    StateStore(crate::capability::CapabilityError),
    StateEncoding(serde_json::Error),
}

/// String form of a role keeps errors independent of future role additions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityRoleName {
    Response,
    Message,
    OutputItem,
    ToolCall,
    Resource,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentity(reason) => write!(f, "invalid identity: {reason}"),
            Self::ResourceIdRequired => f.write_str("resource identity requires a source id"),
            Self::NoGeneratedPrefix => {
                f.write_str("identity role has no configured generated prefix")
            }
            Self::DuplicateSourceIdentity {
                role,
                dialect,
                source_id,
                first_index,
                duplicate_index,
            } => write!(
                f,
                "duplicate {role:?} source id {source_id:?} in {dialect}: logical indexes {first_index} and {duplicate_index} are ambiguous"
            ),
            Self::AmbiguousSourceIdentity(reason) => {
                write!(f, "ambiguous source identity: {reason}")
            }
            Self::Collision(id) => write!(f, "identity collision could not be resolved: {id}"),
            Self::MissingState => f.write_str("identity state is missing"),
            Self::ExpiredState => f.write_str("identity state has expired"),
            Self::Conflict => f.write_str("identity state compare-and-exchange conflicted"),
            Self::StateStore(reason) => write!(f, "identity state store failed: {reason}"),
            Self::StateEncoding(reason) => write!(f, "identity state encoding failed: {reason}"),
        }
    }
}

impl std::error::Error for IdentityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::StateStore(error) => Some(error),
            Self::StateEncoding(error) => Some(error),
            _ => None,
        }
    }
}
