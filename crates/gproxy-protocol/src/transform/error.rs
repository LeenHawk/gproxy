use std::{error::Error, fmt};

/// Why a request, result, or required host operation could not be converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum TransformErrorKind {
    InvalidInput,
    InvalidResult,
    Unsupported,
    MissingMetadata,
    MissingState,
    Conflict,
    Limit,
    Host,
}

/// A conversion failure with an explicit field/operation context and cause.
#[derive(Debug)]
pub struct TransformError {
    kind: TransformErrorKind,
    context: String,
    detail: String,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl TransformError {
    pub fn new(
        kind: TransformErrorKind,
        context: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            context: context.into(),
            detail: detail.into(),
            source: None,
        }
    }

    pub fn with_source(
        kind: TransformErrorKind,
        context: impl Into<String>,
        detail: impl Into<String>,
        source: impl Into<Box<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            kind,
            context: context.into(),
            detail: detail.into(),
            source: Some(source.into()),
        }
    }

    pub fn shape(context: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(TransformErrorKind::InvalidInput, context, detail)
    }

    pub fn unsupported(context: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(TransformErrorKind::Unsupported, context, detail)
    }

    pub fn invalid_result(context: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(TransformErrorKind::InvalidResult, context, detail)
    }

    pub fn missing_metadata(field: impl Into<String>) -> Self {
        Self::new(
            TransformErrorKind::MissingMetadata,
            field,
            "required factual metadata is unavailable",
        )
    }

    pub fn kind(&self) -> TransformErrorKind {
        self.kind
    }
    pub fn context(&self) -> &str {
        &self.context
    }
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.context, self.detail)
    }
}

impl Error for TransformError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

impl From<serde_json::Error> for TransformError {
    fn from(error: serde_json::Error) -> Self {
        let message = error.to_string();
        Self::with_source(TransformErrorKind::InvalidInput, "JSON", message, error)
    }
}

impl From<crate::capability::CapabilityError> for TransformError {
    fn from(error: crate::capability::CapabilityError) -> Self {
        use crate::capability::CapabilityErrorKind;
        let kind = match error.kind() {
            CapabilityErrorKind::Unsupported => TransformErrorKind::Unsupported,
            CapabilityErrorKind::NotFound | CapabilityErrorKind::Expired => {
                TransformErrorKind::Host
            }
            CapabilityErrorKind::Conflict => TransformErrorKind::Conflict,
            CapabilityErrorKind::Limit => TransformErrorKind::Limit,
            CapabilityErrorKind::Invalid => TransformErrorKind::InvalidInput,
            CapabilityErrorKind::Transport | CapabilityErrorKind::Storage => {
                TransformErrorKind::Host
            }
        };
        let message = error.to_string();
        Self::with_source(kind, "host capability", message, error)
    }
}

/// A declared field whose representation changes or cannot be retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum DiagnosticKind {
    /// A source field or content block omitted because it has no target
    /// representation. Other supported parts of the conversion are retained.
    OmittedOptional,
    RepresentationChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub field: String,
    pub kind: DiagnosticKind,
    pub message: String,
}

/// Owned diagnostics for one conversion, never serialized into vendor `rest`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn omitted(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            field: field.into(),
            kind: DiagnosticKind::OmittedOptional,
            message: message.into(),
        });
    }

    pub fn changed(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            field: field.into(),
            kind: DiagnosticKind::RepresentationChanged,
            message: message.into(),
        });
    }
}
