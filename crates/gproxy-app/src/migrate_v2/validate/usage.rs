use std::collections::BTreeSet;

use crate::migrate_v2::model::SourceData;
use crate::migrate_v2::plan::issue;
use crate::migrate_v2::report::ImportIssue;

pub(super) struct References<'a> {
    pub providers: &'a BTreeSet<i64>,
    pub credentials: &'a BTreeSet<i64>,
}

pub(super) fn run(data: &SourceData, issues: &mut Vec<ImportIssue>, refs: References<'_>) {
    for value in &data.usage {
        let usage = &value.value;
        let references = usage
            .provider_id
            .is_some_and(|id| refs.providers.contains(&id))
            && usage
                .credential_id
                .is_some_and(|id| refs.credentials.contains(&id));
        let counters = [
            usage.input_tokens,
            usage.output_tokens,
            usage.image_output_tokens,
            usage.cache_read_tokens,
            usage.cache_creation_5m_tokens,
            usage.cache_creation_30m_tokens,
            usage.cache_creation_1h_tokens,
            usage.latency_ms,
        ]
        .into_iter()
        .all(|value| value >= 0);
        if !references || !counters || !usage.metrics.is_object() {
            issues.push(issue(
                "usage",
                value.id,
                "has a missing provider or credential, negative counter, or invalid metrics object",
            ));
        }
    }
}
