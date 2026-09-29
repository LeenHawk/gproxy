//! What one v3 import did, and — with more weight — what it declined to do.
//!
//! An operator upgrading needs two lists, not one. The first is what arrived,
//! and it is a reassurance. The second is what did not, and it is a **work
//! item**: a v3 deployment whose rules or aliases did not survive is a
//! deployment that behaves differently after the upgrade, and the only thing
//! worse than telling them is not telling them. So every row this migration
//! drops is named individually, with the reason, and the summary the command
//! prints leads with the count of them.

use std::collections::BTreeMap;

/// One v3 row that has no v4 form, named so an operator can rebuild it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dropped {
    /// The v3 table, as an operator sees it in the console.
    pub table: &'static str,
    /// The v3 row, e.g. `id 7 (anthropic)`.
    pub row: String,
    /// Why, in one sentence, ending with what to do instead.
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    /// v4 rows written, keyed by the v4 table they went into.
    migrated: BTreeMap<&'static str, u64>,
    /// v3 rows that did not travel.
    pub dropped: Vec<Dropped>,
    /// Things that travelled but not unchanged — a period that became a
    /// different period, an owner that had to be dropped from a row that
    /// otherwise survived.
    pub warnings: Vec<String>,
}

impl Report {
    pub fn count(&mut self, table: &'static str, rows: u64) {
        if rows > 0 {
            *self.migrated.entry(table).or_default() += rows;
        }
    }

    pub fn drop_row(
        &mut self,
        table: &'static str,
        row: impl Into<String>,
        reason: impl Into<String>,
    ) {
        self.dropped.push(Dropped {
            table,
            row: row.into(),
            reason: reason.into(),
        });
    }

    pub fn warn(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    pub fn rows(&self) -> u64 {
        self.migrated.values().sum()
    }

    pub fn tables(&self) -> impl Iterator<Item = (&'static str, u64)> + '_ {
        self.migrated.iter().map(|(table, rows)| (*table, *rows))
    }

    /// Fold another report into this one, so the configuration half and the
    /// identity half are one answer.
    pub fn absorb(&mut self, other: Report) {
        for (table, rows) in other.migrated {
            *self.migrated.entry(table).or_default() += rows;
        }
        self.dropped.extend(other.dropped);
        self.warnings.extend(other.warnings);
    }

    /// Tell the operator. Counts go to the log; the two lists that are work
    /// items go to standard output, because they are what the person who ran
    /// the command has to read and act on, and a log line is a thing that
    /// scrolls past.
    pub fn announce(&self) {
        for (table, rows) in self.tables() {
            tracing::info!(table, rows, "migrated");
        }
        tracing::info!(
            rows = self.rows(),
            dropped = self.dropped.len(),
            warnings = self.warnings.len(),
            "v3 import complete"
        );

        if !self.warnings.is_empty() {
            println!();
            println!("Changed on the way in ({}):", self.warnings.len());
            for warning in &self.warnings {
                println!("  - {warning}");
            }
        }
        if self.dropped.is_empty() {
            return;
        }
        println!();
        println!(
            "NOT migrated ({} rows). These have no v4 equivalent and have to be",
            self.dropped.len()
        );
        println!("re-created by hand; see the v3-to-v4 page in the documentation.");
        let mut table = "";
        for entry in &self.dropped {
            if entry.table != table {
                table = entry.table;
                println!("  {table}:");
            }
            println!("    - {}: {}", entry.row, entry.reason);
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_add_up_across_tables_and_across_halves() {
        let mut first = Report::default();
        first.count("providers", 2);
        first.count("credentials", 3);
        // A zero is not a table: an empty list should not appear in the summary.
        first.count("routes", 0);

        let mut second = Report::default();
        second.count("providers", 1);
        second.count("users", 4);
        second.warn("something changed");
        second.drop_row("aliases", "id 1 (gpt4)", "no v4 table");

        first.absorb(second);
        assert_eq!(first.rows(), 10);
        assert_eq!(
            first.tables().collect::<Vec<_>>(),
            [("credentials", 3), ("providers", 3), ("users", 4)]
        );
        assert_eq!(first.warnings.len(), 1);
        assert_eq!(first.dropped.len(), 1);
    }

    #[test]
    fn a_dropped_row_keeps_its_table_its_row_and_its_reason() {
        let mut report = Report::default();
        report.drop_row(
            "rules",
            "id 9 (system_text)",
            "v4 rewrites are replacements",
        );
        assert_eq!(
            report.dropped[0],
            Dropped {
                table: "rules",
                row: "id 9 (system_text)".into(),
                reason: "v4 rewrites are replacements".into(),
            }
        );
    }
}
