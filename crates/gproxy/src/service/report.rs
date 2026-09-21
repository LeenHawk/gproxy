//! What a `service` command tells the operator.
//!
//! Three commands with genuinely different things to say — a path and a
//! command line, a supervisor's own status fields, a paragraph about an add-on
//! that has to be installed separately — so this is a label/value list and a
//! set of notes rather than a struct with a field per fact. A struct would
//! force all four platforms to answer the same questions, and they cannot: a
//! Termux boot script has no `ActiveState` and a launchd agent has no
//! `Linger`.
//!
//! It prints to **standard output**, not the log. This is a command an
//! operator ran and is watching; a `tracing` line would be filtered by
//! `--log-filter`, prefixed with a timestamp and a level, and shipped to
//! wherever the journal goes.

/// A label/value list, then any number of paragraphs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    heading: String,
    rows: Vec<(String, String)>,
    notes: Vec<String>,
}

impl Report {
    pub fn new(heading: impl Into<String>) -> Self {
        Self {
            heading: heading.into(),
            ..Self::default()
        }
    }

    /// One fact, with the label an operator would grep for.
    pub fn row(mut self, label: impl Into<String>, value: impl std::fmt::Display) -> Self {
        self.rows.push((label.into(), value.to_string()));
        self
    }

    /// A row only when there is something to put in it. Every platform's
    /// status has fields the supervisor may simply not report.
    pub fn maybe(self, label: impl Into<String>, value: Option<impl std::fmt::Display>) -> Self {
        match value {
            Some(value) => self.row(label, value),
            None => self,
        }
    }

    /// Something that needs a sentence rather than a column.
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn rows(&self) -> &[(String, String)] {
        &self.rows
    }

    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// The value of one row, for a test that wants to assert on a fact rather
    /// than on the formatting of it.
    pub fn value(&self, label: &str) -> Option<&str> {
        self.rows
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, value)| value.as_str())
    }

    pub fn print(&self) {
        println!("{}", self.heading);
        let width = self
            .rows
            .iter()
            .map(|(label, _)| label.chars().count())
            .max()
            .unwrap_or(0);
        for (label, value) in &self.rows {
            println!("  {label:<width$}  {value}");
        }
        for note in &self.notes {
            // Wrapped by hand rather than by a dependency: these are three or
            // four sentences, and a terminal width this process cannot know is
            // a worse guide than a fixed margin.
            println!();
            println!("{}", wrap(note, 76));
        }
    }
}

/// Greedy wrap at `width`, on spaces. Long tokens — a path, a URL — are left
/// over-long rather than broken, because a path split across two lines cannot
/// be copied.
fn wrap(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut column = 0;
    for word in text.split_whitespace() {
        let len = word.chars().count();
        if column > 0 && column + 1 + len > width {
            out.push('\n');
            column = 0;
        } else if column > 0 {
            out.push(' ');
            column += 1;
        }
        out.push_str(word);
        column += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_findable_by_its_label() {
        let report = Report::new("gproxy service")
            .row("unit", "/home/x/.config/systemd/user/gproxy.service")
            .maybe("active", Some("running"))
            .maybe("linger", None::<&str>)
            .note("something worth a sentence");
        assert_eq!(report.value("active"), Some("running"));
        assert_eq!(report.value("linger"), None);
        assert_eq!(report.rows().len(), 2);
        assert_eq!(report.notes().len(), 1);
    }

    #[test]
    fn wrapping_never_breaks_a_path() {
        let path = "/a/very/long/path/that/is/quite/definitely/longer/than/the/margin/allows";
        let wrapped = wrap(&format!("the file is {path} and that is that"), 20);
        assert!(wrapped.lines().any(|line| line == path), "{wrapped}");
    }
}
