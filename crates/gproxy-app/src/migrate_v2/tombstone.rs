use std::collections::{BTreeMap, BTreeSet};

use gproxy_store::records::ProviderInput;
use serde_json::json;

use super::model::{Credential, Legacy, SourceData};
use super::plan::issue;
use super::report::ImportIssue;

pub(super) fn prepare(data: &mut SourceData, issues: &mut Vec<ImportIssue>) {
    let provider_ids = data
        .providers
        .iter()
        .map(|value| value.id)
        .collect::<BTreeSet<_>>();
    let mut provider_names = data
        .providers
        .iter()
        .map(|value| value.value.name.clone())
        .collect::<BTreeSet<_>>();
    let missing_providers = data
        .usage
        .iter()
        .filter_map(|value| value.value.provider_id)
        .chain(data.log_references.iter().filter_map(|value| value.0))
        .filter(|id| !provider_ids.contains(id))
        .collect::<BTreeSet<_>>();
    for id in missing_providers {
        let name = available_provider_name(id, &mut provider_names);
        data.usage_tombstone_providers.push(Legacy {
            id,
            value: ProviderInput {
                name,
                label: Some(format!("Deleted v2 provider #{id} (usage history)")),
                channel: "custom".into(),
                settings: json!({"base_url":"https://deleted.invalid"}),
                credential_strategy: "round_robin".into(),
                proxy_url: None,
                tls_fingerprint: None,
                enabled: false,
            },
        });
    }

    let credential_ids = data
        .credentials
        .iter()
        .map(|value| value.id)
        .collect::<BTreeSet<_>>();
    let mut missing_credentials = BTreeMap::<i64, BTreeSet<i64>>::new();
    for (provider_id, credential_id) in data
        .usage
        .iter()
        .map(|row| (row.value.provider_id, row.value.credential_id))
        .chain(data.log_references.iter().copied())
    {
        let Some(credential_id) = credential_id else {
            continue;
        };
        if !credential_ids.contains(&credential_id) {
            missing_credentials
                .entry(credential_id)
                .or_default()
                .extend(provider_id);
        }
    }
    for (id, providers) in missing_credentials {
        if providers.len() != 1 {
            issues.push(issue(
                "usage_credential_tombstones",
                id,
                "deleted credential has no unique provider reference",
            ));
            continue;
        }
        let provider_id = *providers.first().expect("one provider was validated");
        data.usage_tombstone_credentials.push(Legacy {
            id,
            value: Credential {
                provider_id,
                label: Some(format!("Deleted v2 credential #{id} (usage history)")),
                kind: "api_key".into(),
                stored_secret: json!({}),
                weight: 0,
                rpm_limit: None,
                tpm_limit: None,
                proxy_url: None,
                tls_fingerprint: None,
                enabled: false,
            },
        });
    }
}

fn available_provider_name(id: i64, names: &mut BTreeSet<String>) -> String {
    let base = format!("v2-deleted-provider-{id}");
    let mut name = base.clone();
    let mut suffix = 2;
    while !names.insert(name.clone()) {
        name = format!("{base}-{suffix}");
        suffix += 1;
    }
    name
}
