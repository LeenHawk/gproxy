use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCount {
    pub entity: &'static str,
    pub found: usize,
    pub importable: usize,
    pub imported: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportIssue {
    pub entity: &'static str,
    pub row: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedTable {
    pub table: String,
    pub rows: u64,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2ImportReport {
    pub dry_run: bool,
    pub applied: bool,
    pub already_imported: bool,
    pub counts: Vec<ImportCount>,
    pub existing: Vec<(&'static str, usize)>,
    pub issues: Vec<ImportIssue>,
    pub skipped: Vec<SkippedTable>,
}

impl V2ImportReport {
    pub fn has_blockers(&self) -> bool {
        !self.issues.is_empty() || (!self.dry_run && !self.applied && !self.already_imported)
    }
}

impl std::fmt::Display for V2ImportReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut output = String::new();
        let mode = if self.dry_run {
            "dry run"
        } else if self.applied {
            "applied"
        } else {
            "not applied"
        };
        writeln!(output, "v2 migration: {mode}").expect("write to string");
        for count in &self.counts {
            if self.applied {
                writeln!(
                    output,
                    "  {}: {} imported ({} found)",
                    count.entity, count.imported, count.found
                )
                .expect("write to string");
            } else {
                writeln!(
                    output,
                    "  {}: {} importable ({} found)",
                    count.entity, count.importable, count.found
                )
                .expect("write to string");
            }
        }
        if !self.skipped.is_empty() {
            output.push_str("source rows excluded from the v3 store:\n");
            for skipped in &self.skipped {
                writeln!(
                    output,
                    "  {}: {} rows; {}",
                    skipped.table, skipped.rows, skipped.reason
                )
                .expect("write to string");
            }
        }
        if !self.existing.is_empty() {
            output.push_str("existing target rows:\n");
            for (entity, count) in &self.existing {
                writeln!(output, "  {entity}: {count}").expect("write to string");
            }
        }
        if !self.issues.is_empty() {
            output.push_str("unrecoverable rows:\n");
            for issue in &self.issues {
                writeln!(output, "  {} {}: {}", issue.entity, issue.row, issue.reason)
                    .expect("write to string");
            }
        }
        if self.already_imported {
            output.push_str("this v2 source was already imported; no rows were written\n");
        } else if self.dry_run {
            output.push_str("dry run wrote nothing; rerun with --apply to import\n");
        }
        formatter.write_str(output.trim_end())
    }
}
