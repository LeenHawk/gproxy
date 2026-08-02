//! Field collection and edge-log safety filtering.

use tracing::field::{Field, Visit};

#[derive(Default)]
pub(super) struct Fields(Vec<(String, String)>);

impl Fields {
    fn set(&mut self, field: &Field, value: String) {
        let name = field.name();
        let value = safe_value(name, value);
        if let Some((_, old)) = self.0.iter_mut().find(|(key, _)| key == name) {
            *old = value;
        } else {
            self.0.push((name.to_owned(), value));
        }
    }

    pub(super) fn append_to(&self, output: &mut String) {
        for (name, value) in &self.0 {
            if name != "message" {
                output.push(' ');
                output.push_str(name);
                output.push('=');
                output.push_str(value);
            }
        }
    }

    pub(super) fn message(&self) -> Option<&str> {
        self.0
            .iter()
            .find_map(|(name, value)| (name == "message").then_some(value.as_str()))
    }
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.set(field, format!("{value:?}"));
    }
}

fn safe_value(name: &str, value: String) -> String {
    let name = name.to_ascii_lowercase();
    if [
        "token",
        "secret",
        "password",
        "cookie",
        "authorization",
        "body",
    ]
    .iter()
    .any(|part| name == *part || name.ends_with(&format!("_{part}")))
    {
        "[redacted]".to_owned()
    } else {
        crate::http::telemetry::redact_url_query(&value).into_owned()
    }
}
