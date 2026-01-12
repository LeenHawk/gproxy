use anyhow::{Result, anyhow};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::{self, Duration, Instant};
use toml_edit::DocumentMut;
use tracing::{debug, error};

use crate::config::AppConfig;
use crate::providers::ProvidersConfig;
use crate::storage::ConfigStore;

pub struct FileStorage {
    path: PathBuf,
    tx: mpsc::Sender<WriteRequest>,
    config: ArcSwap<AppConfig>,
    doc: Mutex<Option<DocumentMut>>,
    update_lock: Mutex<()>,
    debounce: Duration,
}

impl FileStorage {
    pub fn new(path: PathBuf, debounce: Duration) -> Result<Self> {
        let (tx, rx) = mpsc::channel(32);
        let writer_path = path.clone();
        tokio::spawn(async move {
            write_worker(writer_path, rx, debounce).await;
        });
        Ok(Self {
            path,
            tx,
            config: ArcSwap::from_pointee(AppConfig::default()),
            doc: Mutex::new(None),
            update_lock: Mutex::new(()),
            debounce,
        })
    }

    pub fn debounce(&self) -> Duration {
        self.debounce
    }

    pub async fn providers_get<R, F>(&self, read: F) -> Result<R>
    where
        F: FnOnce(&ProvidersConfig) -> R + Send,
    {
        let config = self.config.load_full();
        Ok(read(&config.providers))
    }

    pub async fn providers_update<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut ProvidersConfig, Option<&mut DocumentMut>) -> Result<()> + Send,
    {
        let mut config = (*self.config.load_full()).clone();
        self.with_document_mut(|doc| update(&mut config.providers, Some(doc)))
            .await?;
        self.config.store(Arc::new(config));
        Ok(())
    }
}

#[async_trait]
impl ConfigStore for FileStorage {
    async fn get_app_config(&self) -> Result<AppConfig> {
        Ok((*self.config.load_full()).clone())
    }

    async fn load_app_config(&self) -> Result<AppConfig> {
        let contents = match fs::read_to_string(&self.path).await {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let config = AppConfig::default();
                let contents = toml::to_string(&config)?;
                fs::write(&self.path, contents.as_bytes()).await?;
                let mut doc_guard = self.doc.lock().await;
                *doc_guard = Some(contents.parse()?);
                self.config.store(Arc::new(config.clone()));
                return Ok(config);
            }
            Err(err) => return Err(err.into()),
        };

        let config: AppConfig = toml::from_str(&contents)?;
        let mut doc_guard = self.doc.lock().await;
        *doc_guard = Some(contents.parse()?);
        self.config.store(Arc::new(config.clone()));
        Ok(config)
    }

    async fn save_app_config(&self, config: &AppConfig) -> Result<()> {
        self.config.store(Arc::new(config.clone()));
        self.tx
            .send(WriteRequest::Write(Box::new(config.clone())))
            .await
            .map_err(|_| anyhow!("storage writer task closed"))?;
        Ok(())
    }

    async fn flush_app_config(&self) -> Result<()> {
        debug!("file storage flush requested");
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(WriteRequest::Flush(tx))
            .await
            .map_err(|_| anyhow!("storage writer task closed"))?;
        let result = rx
            .await
            .map_err(|_| anyhow!("storage writer task closed"))?;
        if result.is_ok() {
            debug!("file storage flush completed");
        }
        result
    }
}

enum WriteRequest {
    Write(Box<AppConfig>),
    WriteRaw(String),
    Flush(oneshot::Sender<Result<()>>),
}

async fn write_worker(path: PathBuf, mut rx: mpsc::Receiver<WriteRequest>, debounce: Duration) {
    while let Some(request) = rx.recv().await {
        let mut flush_waiters: Vec<oneshot::Sender<Result<()>>> = Vec::new();
        let mut pending = match request {
            WriteRequest::Write(config) => Some(WritePayload::Config(config)),
            WriteRequest::WriteRaw(raw) => Some(WritePayload::Raw(raw)),
            WriteRequest::Flush(tx) => {
                let _ = tx.send(Ok(()));
                continue;
            }
        };

        if debounce.is_zero() {
            loop {
                match rx.try_recv() {
                    Ok(WriteRequest::Write(config)) => {
                        pending = Some(WritePayload::Config(config));
                    }
                    Ok(WriteRequest::WriteRaw(raw)) => {
                        pending = Some(WritePayload::Raw(raw));
                    }
                    Ok(WriteRequest::Flush(tx)) => {
                        flush_waiters.push(tx);
                        break;
                    }
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
                            Some(WriteRequest::Write(config)) => {
                                pending = Some(WritePayload::Config(config));
                            }
                            Some(WriteRequest::WriteRaw(raw)) => {
                                pending = Some(WritePayload::Raw(raw));
                            }
                            Some(WriteRequest::Flush(tx)) => {
                                flush_waiters.push(tx);
                                break;
                            }
                            None => break,
                        }
                    }
                }
            }
        }

        if let Some(payload) = pending {
            let result = match payload {
                WritePayload::Config(config) => write_config(&path, config.as_ref()).await,
                WritePayload::Raw(raw) => fs::write(&path, raw).await.map_err(|err| err.into()),
            };
            if let Err(err) = &result {
                error!("file storage write failed: {}", err);
            }
            let err_msg = result.as_ref().err().map(|err| err.to_string());
            for waiter in flush_waiters {
                let send_result = match &err_msg {
                    Some(msg) => Err(anyhow!(msg.clone())),
                    None => Ok(()),
                };
                let _ = waiter.send(send_result);
            }
        }
    }
}

async fn write_config(path: &PathBuf, config: &AppConfig) -> Result<()> {
    let contents = toml::to_string(config)?;
    fs::write(path, contents).await?;
    Ok(())
}

enum WritePayload {
    Config(Box<AppConfig>),
    Raw(String),
}

impl FileStorage {
    pub(crate) async fn with_document_mut<F, R>(&self, apply: F) -> Result<R>
    where
        F: FnOnce(&mut DocumentMut) -> Result<R>,
    {
        let mut doc_guard = self.doc.lock().await;
        if doc_guard.is_none() {
            let contents = match fs::read_to_string(&self.path).await {
                Ok(contents) => contents,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    let config = AppConfig::default();
                    toml::to_string(&config)?
                }
                Err(err) => return Err(err.into()),
            };
            *doc_guard = Some(contents.parse()?);
        }
        let doc = doc_guard.as_mut().expect("document cached");
        let result = apply(doc)?;
        let contents = doc.to_string();
        self.tx
            .send(WriteRequest::WriteRaw(contents))
            .await
            .map_err(|_| anyhow!("storage writer task closed"))?;
        Ok(result)
    }

    pub(crate) async fn lock_update(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.update_lock.lock().await
    }

    pub(crate) fn update_cached_config<F>(&self, update: F) -> AppConfig
    where
        F: FnOnce(&mut AppConfig),
    {
        let mut next = (*self.config.load_full()).clone();
        update(&mut next);
        self.config.store(Arc::new(next.clone()));
        next
    }
}
