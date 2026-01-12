use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::time::{self, Duration, Instant};

use crate::providers::credential_status::ProviderKind;
use crate::providers::usage::{UsageRecord, UsageViewRecord};

use super::{UsageBackend, usage_views_for_provider};
use super::view::UsageViewCache;

pub struct FileUsageStore {
    cache: UsageViewCache,
    tx: mpsc::Sender<WriteRequest>,
}

impl FileUsageStore {
    pub async fn new(
        path: PathBuf,
        data_dir: Option<PathBuf>,
        debounce_secs: u64,
        anchor_ts: i64,
    ) -> Result<Self> {
        let dir = data_dir
            .or_else(|| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        fs::create_dir_all(&dir).await?;
        let usage_path = dir.join("usage.jsonl");
        let view_path = dir.join("usage_view.jsonl");
        let cache = UsageViewCache::new(anchor_ts);
        load_view_cache(&cache, &usage_path, &view_path).await?;
        let (tx, rx) = mpsc::channel(64);
        let cache_clone = cache.clone();
        let usage_path_clone = usage_path.clone();
        let view_path_clone = view_path.clone();
        let debounce = Duration::from_secs(debounce_secs);
        tokio::spawn(async move {
            write_worker(
                usage_path_clone,
                view_path_clone,
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
impl UsageBackend for FileUsageStore {
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
    usage_path: PathBuf,
    view_path: PathBuf,
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
            && let Err(err) = append_usage_records(&usage_path, &pending_records).await {
                tracing::error!("usage append failed: {}", err);
            }
        if flush_view
            && let Err(err) = write_view_snapshot(&view_path, &cache).await {
                tracing::error!("usage view flush failed: {}", err);
            }
        if !flush_waiters.is_empty() {
            let result = write_view_snapshot(&view_path, &cache).await;
            for waiter in flush_waiters {
                let _ = waiter.send(result.as_ref().map(|_| ()).map_err(|err| anyhow!(err.to_string())));
            }
        }
    }
}

async fn append_usage_records(path: &Path, records: &[UsageRecord]) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    for record in records {
        let line = serde_json::to_string(record)?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
    }
    Ok(())
}

async fn write_view_snapshot(path: &Path, cache: &UsageViewCache) -> Result<()> {
    let snapshot = cache.snapshot().await;
    let mut buf = String::new();
    for record in snapshot {
        let line = serde_json::to_string(&record)?;
        buf.push_str(&line);
        buf.push('\n');
    }
    fs::write(path, buf).await?;
    Ok(())
}

async fn load_view_cache(
    cache: &UsageViewCache,
    usage_path: &Path,
    view_path: &Path,
) -> Result<()> {
    if fs::metadata(view_path).await.is_ok() {
        let contents = fs::read_to_string(view_path).await?;
        for line in contents.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let record: UsageViewRecord = serde_json::from_str(line)?;
            cache.insert_view_record(record).await;
        }
        return Ok(());
    }
    if fs::metadata(usage_path).await.is_ok() {
        let contents = fs::read_to_string(usage_path).await?;
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
