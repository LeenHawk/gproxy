use anyhow::{Result, anyhow};
use s3::creds::Credentials;
use s3::{Bucket, Region};
use tokio::sync::mpsc;
use tokio::time::{self, Duration, Instant};
use url::Url;

use crate::providers::credential_status::ProviderKind;
use crate::providers::usage::{UsageRecord, UsageViewRecord};
use crate::storage::StorageSettings;

use super::{UsageBackend, usage_views_for_provider};
use super::view::UsageViewCache;

pub struct S3UsageStore {
    cache: UsageViewCache,
    tx: mpsc::Sender<WriteRequest>,
}

impl S3UsageStore {
    pub async fn connect(settings: &StorageSettings, anchor_ts: i64) -> Result<Self> {
        let StorageSettings::S3 {
            bucket,
            region,
            access_key,
            secret_key,
            endpoint,
            path_style,
            session_token,
            use_tls,
            path,
            debounce_secs,
        } = settings
        else {
            return Err(anyhow!("usage storage settings mismatch for s3"));
        };

        let credentials = Credentials::new(
            Some(access_key),
            Some(secret_key),
            None,
            session_token.as_deref(),
            None,
        )?;

        let region = match endpoint {
            Some(endpoint) => Region::Custom {
                region: region.clone(),
                endpoint: normalize_endpoint(endpoint, *use_tls)?,
            },
            None => region
                .parse()
                .map_err(|err| anyhow!("invalid s3 region: {}", err))?,
        };

        let mut bucket = Bucket::new(bucket, region, credentials)?;
        if path_style.unwrap_or(false) {
            bucket.set_path_style();
        }

        let (usage_key, view_key) = derive_keys(path);
        let cache = UsageViewCache::new(anchor_ts);
        load_view_cache(&bucket, &usage_key, &view_key, &cache).await?;

        let (tx, rx) = mpsc::channel(64);
        let cache_clone = cache.clone();
        let bucket_clone = bucket.clone();
        let usage_key_clone = usage_key.clone();
        let view_key_clone = view_key.clone();
        let debounce = Duration::from_secs(*debounce_secs);
        tokio::spawn(async move {
            write_worker(
                bucket_clone,
                usage_key_clone,
                view_key_clone,
                cache_clone,
                rx,
                debounce,
            )
            .await;
        });

        Ok(Self { cache, tx })
    }
}

#[async_trait::async_trait]
impl UsageBackend for S3UsageStore {
    async fn record(&self, record: UsageRecord) -> Result<()> {
        self.cache.apply_record_all(usage_views_for_provider(record.provider), &record).await;
        self.tx
            .send(WriteRequest::Append(Box::new(record)))
            .await
            .map_err(|_| anyhow!("usage writer task closed"))?;
        self.tx
            .send(WriteRequest::FlushView)
            .await
            .map_err(|_| anyhow!("usage writer task closed"))?;
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(WriteRequest::Flush(tx))
            .await
            .map_err(|_| anyhow!("usage writer task closed"))?;
        rx.await
            .map_err(|_| anyhow!("usage writer task closed"))?
    }

    async fn set_anchor(
        &self,
        provider: ProviderKind,
        view_name: &str,
        anchor_ts: i64,
    ) -> Result<()> {
        self.cache.set_anchor(provider, view_name, anchor_ts).await;
        Ok(())
    }

    async fn query_by_api_key(&self, api_key: &str) -> Result<Vec<UsageViewRecord>> {
        Ok(self.cache.query_by_api_key(api_key).await)
    }

    async fn query_by_provider_credential(
        &self,
        provider: ProviderKind,
        credential_id: &str,
    ) -> Result<Vec<UsageViewRecord>> {
        Ok(self
            .cache
            .query_by_provider_credential(provider, credential_id)
            .await)
    }
}

#[derive(Debug)]
enum WriteRequest {
    Append(Box<UsageRecord>),
    FlushView,
    Flush(tokio::sync::oneshot::Sender<Result<()>>),
}

async fn write_worker(
    bucket: Box<Bucket>,
    usage_key: String,
    view_key: String,
    cache: UsageViewCache,
    mut rx: mpsc::Receiver<WriteRequest>,
    debounce: Duration,
) {
    while let Some(request) = rx.recv().await {
        let mut pending_records: Vec<UsageRecord> = Vec::new();
        let mut flush_view = false;
        let mut flush_waiters: Vec<tokio::sync::oneshot::Sender<Result<()>>> = Vec::new();
        match request {
            WriteRequest::Append(record) => pending_records.push(*record),
            WriteRequest::FlushView => flush_view = true,
            WriteRequest::Flush(tx) => flush_waiters.push(tx),
        }

        if debounce.is_zero() {
            loop {
                match rx.try_recv() {
                    Ok(WriteRequest::Append(record)) => pending_records.push(*record),
                    Ok(WriteRequest::FlushView) => flush_view = true,
                    Ok(WriteRequest::Flush(tx)) => flush_waiters.push(tx),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => break,
                }
            }
        } else {
            let deadline = Instant::now() + debounce;
            loop {
                let sleep = time::sleep_until(deadline);
                tokio::pin!(sleep);
                tokio::select! {
                    _ = &mut sleep => break,
                    message = rx.recv() => {
                        match message {
                            Some(WriteRequest::Append(record)) => pending_records.push(*record),
                            Some(WriteRequest::FlushView) => flush_view = true,
                            Some(WriteRequest::Flush(tx)) => flush_waiters.push(tx),
                            None => break,
                        }
                    }
                }
            }
        }

        if !pending_records.is_empty()
            && let Err(err) = append_usage_records(&bucket, &usage_key, &pending_records).await {
                tracing::error!("usage s3 append failed: {}", err);
            }
        if flush_view
            && let Err(err) = write_view_snapshot(&bucket, &view_key, &cache).await {
                tracing::error!("usage view s3 flush failed: {}", err);
            }
        if !flush_waiters.is_empty() {
            let result = write_view_snapshot(&bucket, &view_key, &cache).await;
            for waiter in flush_waiters {
                let _ = waiter.send(result.as_ref().map(|_| ()).map_err(|err| anyhow!(err.to_string())));
            }
        }
    }
}

async fn append_usage_records(
    bucket: &Bucket,
    key: &str,
    records: &[UsageRecord],
) -> Result<()> {
    let existing = if bucket.object_exists(key).await? {
        let response = bucket.get_object(key).await?;
        response
            .to_string()
            .map_err(|err| anyhow!("s3 usage is not valid utf-8: {}", err))?
    } else {
        String::new()
    };
    let mut buf = existing;
    for record in records {
        let line = serde_json::to_string(record)?;
        buf.push_str(&line);
        buf.push('\n');
    }
    bucket.put_object(key, buf.as_bytes()).await?;
    Ok(())
}

async fn write_view_snapshot(bucket: &Bucket, key: &str, cache: &UsageViewCache) -> Result<()> {
    let snapshot = cache.snapshot().await;
    let mut buf = String::new();
    for record in snapshot {
        let line = serde_json::to_string(&record)?;
        buf.push_str(&line);
        buf.push('\n');
    }
    bucket.put_object(key, buf.as_bytes()).await?;
    Ok(())
}

async fn load_view_cache(
    bucket: &Bucket,
    usage_key: &str,
    view_key: &str,
    cache: &UsageViewCache,
) -> Result<()> {
    if bucket.object_exists(view_key).await? {
        let response = bucket.get_object(view_key).await?;
        let contents = response
            .to_string()
            .map_err(|err| anyhow!("s3 usage view is not valid utf-8: {}", err))?;
        for line in contents.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let record: UsageViewRecord = serde_json::from_str(line)?;
            cache.insert_view_record(record).await;
        }
        return Ok(());
    }
    if bucket.object_exists(usage_key).await? {
        let response = bucket.get_object(usage_key).await?;
        let contents = response
            .to_string()
            .map_err(|err| anyhow!("s3 usage is not valid utf-8: {}", err))?;
        for line in contents.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let record: UsageRecord = serde_json::from_str(line)?;
            cache.apply_record_all(usage_views_for_provider(record.provider), &record).await;
        }
    }
    Ok(())
}

fn normalize_endpoint(endpoint: &Url, use_tls: Option<bool>) -> Result<String> {
    let scheme = match use_tls.unwrap_or_else(|| endpoint.scheme() == "https") {
        true => "https",
        false => "http",
    };
    let host = endpoint.host_str().ok_or_else(|| anyhow!("missing s3 endpoint host"))?;
    let port = endpoint
        .port()
        .map(|port| format!(":{}", port))
        .unwrap_or_default();
    Ok(format!("{}://{}{}", scheme, host, port))
}

fn derive_keys(path: &str) -> (String, String) {
    let trimmed = path.trim_matches('/');
    let prefix = trimmed.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let usage_key = if prefix.is_empty() {
        "usage.jsonl".to_string()
    } else {
        format!("{}/usage.jsonl", prefix)
    };
    let view_key = if prefix.is_empty() {
        "usage_view.jsonl".to_string()
    } else {
        format!("{}/usage_view.jsonl", prefix)
    };
    (usage_key, view_key)
}
