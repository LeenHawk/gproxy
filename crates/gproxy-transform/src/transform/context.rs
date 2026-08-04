use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use crate::protocol::OperationKey;

/// Classification of a non-fatal semantic transform diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformDiagnosticKind {
    /// The source field has no representation in the target protocol.
    UnsupportedField,
    /// The field can only be approximated or is intentionally dropped.
    LossyField,
}

/// A structured, non-fatal semantic loss reported by a transform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformDiagnostic {
    /// Diagnostic category suitable for programmatic policy decisions.
    pub kind: TransformDiagnosticKind,
    /// Provider-relative semantic field path affected by the conversion.
    pub field: String,
    /// Human-readable explanation of the loss or approximation.
    pub reason: String,
}

impl TransformDiagnostic {
    /// Report a source field with no target-protocol representation.
    pub fn unsupported(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            kind: TransformDiagnosticKind::UnsupportedField,
            field: field.into(),
            reason: reason.into(),
        }
    }

    /// Report a source field that was approximated or intentionally dropped.
    pub fn lossy(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            kind: TransformDiagnosticKind::LossyField,
            field: field.into(),
            reason: reason.into(),
        }
    }
}

/// A transformed value together with all non-fatal semantic diagnostics
/// produced while creating it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformOutput<T> {
    /// Converted value.
    pub value: T,
    /// Non-fatal semantic diagnostics produced by this conversion call.
    pub diagnostics: Vec<TransformDiagnostic>,
}

impl<T> TransformOutput<T> {
    /// Pair a converted value with its diagnostics.
    pub fn new(value: T, diagnostics: Vec<TransformDiagnostic>) -> Self {
        Self { value, diagnostics }
    }

    /// Discard diagnostics and return only the converted value.
    pub fn into_value(self) -> T {
        self.value
    }

    /// Split the converted value from its diagnostics.
    pub fn into_parts(self) -> (T, Vec<TransformDiagnostic>) {
        (self.value, self.diagnostics)
    }
}

type DiagnosticSink = Arc<Mutex<Vec<TransformDiagnostic>>>;

thread_local! {
    static DIAGNOSTIC_SCOPES: RefCell<Vec<DiagnosticSink>> = const { RefCell::new(Vec::new()) };
}

/// Per-call transform settings.
///
/// `path`/`query` carry the INBOUND request target (provider-relative, as the
/// client sent it) for transforms that need more than the body — e.g. the
/// list-models query conversion. They are filled on the request direction via
/// [`with_request`](Self::with_request); response-direction contexts leave
/// them empty.
#[derive(Debug, Clone)]
pub struct TransformContext {
    pub source: OperationKey,
    pub target: OperationKey,
    pub path: String,
    pub query: Option<String>,
    diagnostics: DiagnosticSink,
}

impl PartialEq for TransformContext {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && self.target == other.target
            && self.path == other.path
            && self.query == other.query
    }
}

impl Eq for TransformContext {}

impl TransformContext {
    pub fn new(source: OperationKey, target: OperationKey) -> Self {
        Self {
            source,
            target,
            path: String::new(),
            query: None,
            diagnostics: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Attach the inbound request target (request-direction contexts).
    pub fn with_request(mut self, path: &str, query: Option<&str>) -> Self {
        self.path = path.to_owned();
        self.query = query.map(str::to_owned);
        self
    }

    /// Return a snapshot of diagnostics reported through this context.
    pub fn diagnostics(&self) -> Vec<TransformDiagnostic> {
        self.diagnostics
            .lock()
            .expect("transform diagnostic mutex poisoned")
            .clone()
    }

    /// Drain diagnostics reported through this context.
    pub fn take_diagnostics(&self) -> Vec<TransformDiagnostic> {
        std::mem::take(
            &mut *self
                .diagnostics
                .lock()
                .expect("transform diagnostic mutex poisoned"),
        )
    }

    pub(crate) fn isolated(&self) -> Self {
        let mut isolated = self.clone();
        isolated.diagnostics = Arc::new(Mutex::new(Vec::new()));
        isolated
    }

    pub(crate) fn scope<T>(&self, f: impl FnOnce() -> T) -> T {
        struct ScopeGuard;
        impl Drop for ScopeGuard {
            fn drop(&mut self) {
                DIAGNOSTIC_SCOPES.with(|scopes| {
                    scopes.borrow_mut().pop();
                });
            }
        }

        DIAGNOSTIC_SCOPES.with(|scopes| scopes.borrow_mut().push(self.diagnostics.clone()));
        let guard = ScopeGuard;
        let output = f();
        drop(guard);
        output
    }
}

pub(crate) fn report_diagnostic(diagnostic: TransformDiagnostic) {
    let sink = DIAGNOSTIC_SCOPES.with(|scopes| scopes.borrow().last().cloned());
    if let Some(sink) = sink {
        sink.lock()
            .expect("transform diagnostic mutex poisoned")
            .push(diagnostic);
    } else {
        tracing::warn!(
            kind = ?diagnostic.kind,
            field = %diagnostic.field,
            reason = %diagnostic.reason,
            "semantic loss outside a diagnostic transform scope"
        );
    }
}

pub(crate) fn report_lossy(field: impl Into<String>, reason: impl Into<String>) {
    report_diagnostic(TransformDiagnostic::lossy(field, reason));
}

pub(crate) fn report_unsupported(field: impl Into<String>, reason: impl Into<String>) {
    report_diagnostic(TransformDiagnostic::unsupported(field, reason));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation};

    #[test]
    fn diagnostic_scopes_are_isolated_and_structured() {
        let key = OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        );
        let context = TransformContext::new(key, key);
        let isolated = context.isolated();
        isolated.scope(|| report_lossy("input.cache_control", "not representable"));

        assert!(context.diagnostics().is_empty());
        assert_eq!(
            isolated.take_diagnostics(),
            vec![TransformDiagnostic::lossy(
                "input.cache_control",
                "not representable"
            )]
        );
        assert!(isolated.diagnostics().is_empty());
    }
}
