use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use arc_swap::ArcSwap;
use http::StatusCode;
use rand::Rng;

use crate::disallow::{DisallowEntry, DisallowKey, DisallowMark, DisallowRecord, DisallowScope};
use crate::response::UpstreamPassthroughError;
use crate::state::{ProviderStateEvent, StateSink};

#[derive(Debug)]
pub struct CredentialEntry<C> {
    pub id: String,
    pub enabled: bool,
    pub weight: u32,
    pub value: Arc<C>,
}

impl<C> Clone for CredentialEntry<C> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            enabled: self.enabled,
            weight: self.weight,
            value: Arc::clone(&self.value),
        }
    }
}

impl<C> CredentialEntry<C> {
    pub fn new(id: impl Into<String>, enabled: bool, weight: u32, value: C) -> Self {
        Self {
            id: id.into(),
            enabled,
            weight,
            value: Arc::new(value),
        }
    }

    pub fn value(&self) -> &C {
        &self.value
    }
}

#[derive(Debug)]
pub struct PoolSnapshot<C> {
    pub credentials: Arc<Vec<CredentialEntry<C>>>,
    pub disallow: Arc<HashMap<DisallowKey, DisallowEntry>>,
}

impl<C> PoolSnapshot<C> {
    pub fn new(
        credentials: Vec<CredentialEntry<C>>,
        disallow: HashMap<DisallowKey, DisallowEntry>,
    ) -> Self {
        Self {
            credentials: Arc::new(credentials),
            disallow: Arc::new(disallow),
        }
    }

    pub fn empty() -> Self {
        Self {
            credentials: Arc::new(Vec::new()),
            disallow: Arc::new(HashMap::new()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AttemptFailure {
    pub passthrough: UpstreamPassthroughError,
    pub mark: Option<DisallowMark>,
}

pub struct CredentialPool<C> {
    provider_name: Arc<str>,
    snapshot: ArcSwap<PoolSnapshot<C>>,
    sink: Option<Arc<dyn StateSink>>,
}

impl<C> std::fmt::Debug for CredentialPool<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let snapshot = self.snapshot.load();
        f.debug_struct("CredentialPool")
            .field("provider_name", &self.provider_name)
            .field("credential_count", &snapshot.credentials.len())
            .field("disallow_count", &snapshot.disallow.len())
            .finish()
    }
}
impl<C> CredentialPool<C> {
    pub fn new(
        provider_name: impl Into<Arc<str>>,
        snapshot: PoolSnapshot<C>,
        sink: Option<Arc<dyn StateSink>>,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            snapshot: ArcSwap::new(Arc::new(snapshot)),
            sink,
        }
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<C>) {
        self.snapshot.store(Arc::new(snapshot));
    }

    pub fn snapshot(&self) -> Arc<PoolSnapshot<C>> {
        self.snapshot.load_full()
    }

    pub async fn execute<T, F, Fut>(
        &self,
        scope_hint: DisallowScope,
        mut f: F,
    ) -> Result<T, UpstreamPassthroughError>
    where
        F: FnMut(CredentialEntry<C>) -> Fut,
        Fut: std::future::Future<Output = Result<T, AttemptFailure>> + Send,
    {
        let snapshot = self.snapshot.load_full();
        let now = SystemTime::now();
        let mut last_error: Option<UpstreamPassthroughError> = None;

        let mut candidates: Vec<(CredentialEntry<C>, u32)> = snapshot
            .credentials
            .iter()
            .filter(|credential| credential.enabled)
            .filter(|credential| !self.is_disallowed(&snapshot, &credential.id, &scope_hint, now))
            .map(|credential| (credential.clone(), credential.weight))
            .collect();

        while !candidates.is_empty() {
            let weights: Vec<u32> = candidates.iter().map(|(_, weight)| *weight).collect();
            let index = pick_weighted_index(&weights);
            let (credential, _) = candidates.swap_remove(index);

            match f(credential.clone()).await {
                Ok(output) => return Ok(output),
                Err(failure) => {
                    if let Some(mark) = failure.mark.clone() {
                        self.apply_mark(&credential.id, mark).await;
                        last_error = Some(failure.passthrough);
                        continue;
                    }

                    return Err(failure.passthrough);
                }
            }
        }

        if let Some(error) = last_error {
            return Err(error);
        }

        Err(UpstreamPassthroughError::service_unavailable(
            "no credential available",
        ))
    }

    pub async fn execute_for_id<T, F, Fut>(
        &self,
        credential_id: &str,
        scope_hint: DisallowScope,
        mut f: F,
    ) -> Result<T, UpstreamPassthroughError>
    where
        F: FnMut(CredentialEntry<C>) -> Fut,
        Fut: std::future::Future<Output = Result<T, AttemptFailure>> + Send,
    {
        let snapshot = self.snapshot.load_full();
        let now = SystemTime::now();

        let Some(credential) = snapshot
            .credentials
            .iter()
            .find(|entry| entry.id == credential_id)
            .cloned()
        else {
            return Err(UpstreamPassthroughError::from_status(
                StatusCode::NOT_FOUND,
                "credential not found",
            ));
        };

        if !credential.enabled {
            return Err(UpstreamPassthroughError::from_status(
                StatusCode::FORBIDDEN,
                "credential disabled",
            ));
        }

        if self.is_disallowed(&snapshot, &credential.id, &scope_hint, now) {
            return Err(UpstreamPassthroughError::from_status(
                StatusCode::FORBIDDEN,
                "credential disallowed",
            ));
        }

        match f(credential.clone()).await {
            Ok(output) => Ok(output),
            Err(failure) => {
                if let Some(mark) = failure.mark.clone() {
                    self.apply_mark(&credential.id, mark).await;
                }
                Err(failure.passthrough)
            }
        }
    }

    fn is_disallowed(
        &self,
        snapshot: &PoolSnapshot<C>,
        credential_id: &str,
        scope_hint: &DisallowScope,
        now: SystemTime,
    ) -> bool {
        let all_key = DisallowKey::new(credential_id.to_string(), DisallowScope::AllModels);
        if let Some(entry) = snapshot.disallow.get(&all_key)
            && entry.is_active(now)
        {
            return true;
        }

        let model_key = match scope_hint {
            DisallowScope::AllModels => None,
            DisallowScope::Model(model) => Some(DisallowKey::new(
                credential_id.to_string(),
                DisallowScope::Model(model.clone()),
            )),
        };

        if let Some(key) = model_key
            && let Some(entry) = snapshot.disallow.get(&key)
        {
            return entry.is_active(now);
        }

        false
    }

    async fn apply_mark(&self, credential_id: &str, mark: DisallowMark) {
        let now = SystemTime::now();
        let until = mark
            .duration
            .and_then(|duration| now.checked_add(duration));

        let entry = DisallowEntry {
            level: mark.level,
            until,
            reason: mark.reason.clone(),
            updated_at: now,
        };

        let key = DisallowKey::new(credential_id.to_string(), mark.scope.clone());

        self.snapshot.rcu(|current| {
            let mut disallow = HashMap::with_capacity(current.disallow.len() + 1);
            for (existing_key, existing_entry) in current.disallow.iter() {
                if existing_entry.is_active(now) {
                    disallow.insert(existing_key.clone(), existing_entry.clone());
                }
            }
            disallow.insert(key.clone(), entry.clone());

            Arc::new(PoolSnapshot {
                credentials: current.credentials.clone(),
                disallow: Arc::new(disallow),
            })
        });

        if let Some(sink) = &self.sink {
            let record = DisallowRecord {
                provider: self.provider_name.to_string(),
                credential_id: credential_id.to_string(),
                scope: mark.scope,
                level: mark.level,
                until,
                reason: mark.reason,
                updated_at: now,
            };
            sink.submit(ProviderStateEvent::UpsertDisallow(record))
                .await;
        }
    }
}

fn pick_weighted_index(weights: &[u32]) -> usize {
    if weights.is_empty() {
        return 0;
    }

    let total: u64 = weights.iter().map(|weight| *weight as u64).sum();
    if total == 0 {
        return rand::rng().random_range(0..weights.len());
    }

    let mut roll = rand::rng().random_range(0..total);
    for (index, weight) in weights.iter().enumerate() {
        let weight = *weight as u64;
        if roll < weight {
            return index;
        }
        roll -= weight;
    }

    weights.len() - 1
}
