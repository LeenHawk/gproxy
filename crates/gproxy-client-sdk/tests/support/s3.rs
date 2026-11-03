use file::opendal::{
    Capability, HttpBody, HttpTransport, HttpTransporter, OperationContext, raw::*,
};
use gproxy_client_sdk::s3::{S3AssetConfig, S3AssetResolver};
use gproxy_file as file;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

#[derive(Debug, Clone)]
pub struct Stored {
    pub bytes: Vec<u8>,
    pub mime: String,
}
#[derive(Debug, Clone)]
pub struct Request {
    pub method: http::Method,
    pub uri: http::Uri,
    pub mime: Option<String>,
}
#[derive(Debug, Clone, Default)]
pub struct Transport {
    pub objects: Arc<Mutex<HashMap<String, Stored>>>,
    pub requests: Arc<Mutex<Vec<Request>>>,
    pub fail: Arc<Mutex<Option<(http::Method, u16)>>>,
}
impl HttpTransport for Transport {
    async fn fetch(
        &self,
        request: http::Request<file::Buffer>,
    ) -> file::Result<http::Response<HttpBody>> {
        assert!(
            request
                .headers()
                .get("authorization")
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("AWS4-HMAC-SHA256 ")
        );
        let path = request.uri().path().to_owned();
        let method = request.method().clone();
        let mime = request
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap().to_owned());
        self.requests.lock().unwrap().push(Request {
            method: method.clone(),
            uri: request.uri().clone(),
            mime: mime.clone(),
        });
        let failure = self
            .fail
            .lock()
            .unwrap()
            .clone()
            .filter(|(m, _)| m == method);
        let (status, data, size, mime): (u16, Vec<u8>, usize, Option<String>) =
            if let Some((_, status)) = failure {
                (status, Vec::new(), 0, None)
            } else {
                let mut objects = self.objects.lock().unwrap();
                match method {
                    http::Method::HEAD => match objects.get(&path) {
                        Some(object) => (
                            200,
                            Vec::new(),
                            object.bytes.len(),
                            Some(object.mime.clone()),
                        ),
                        None => (404, Vec::new(), 0, None),
                    },
                    http::Method::PUT => {
                        let bytes = request.body().to_vec();
                        objects.insert(
                            path,
                            Stored {
                                bytes,
                                mime: mime.unwrap_or_default(),
                            },
                        );
                        (200, Vec::new(), 0, None)
                    }
                    _ => {
                        panic!(
                            "unexpected storage request {method}; resolver must not download media"
                        )
                    }
                }
            };
        let body = HttpBody::new(
            futures_util::stream::iter([Ok(file::Buffer::from(data))]),
            None,
        );
        let mut response = http::Response::builder()
            .status(status)
            .header("content-length", size)
            .header("etag", "\"test-etag\"");
        if let Some(mime) = mime {
            response = response.header("content-type", mime);
        }
        Ok(response.body(body).unwrap())
    }
}
#[derive(Debug, Default)]
pub struct Probe {
    pub presigns: AtomicUsize,
    pub failure: Mutex<Option<file::ErrorKind>>,
    pub require_header: AtomicBool,
}
#[derive(Debug)]
struct CountLayer(Arc<Probe>);
impl Layer for CountLayer {
    fn apply_service(&self, inner: Servicer) -> Servicer {
        Arc::new(CountService {
            inner,
            probe: self.0.clone(),
        })
    }
}
#[derive(Debug)]
struct CountService {
    inner: Servicer,
    probe: Arc<Probe>,
}
impl Service for CountService {
    type Reader = oio::Reader;
    type Writer = oio::Writer;
    type Lister = oio::Lister;
    type Deleter = oio::Deleter;
    type Copier = oio::Copier;
    type Composer = oio::Composer;
    fn info(&self) -> ServiceInfo {
        self.inner.info()
    }
    fn capability(&self) -> Capability {
        self.inner.capability()
    }
    async fn create_dir(
        &self,
        ctx: &OperationContext,
        path: &str,
        args: OpCreateDir,
    ) -> file::Result<RpCreateDir> {
        self.inner.create_dir(ctx, path, args).await
    }
    async fn stat(&self, ctx: &OperationContext, path: &str, args: OpStat) -> file::Result<RpStat> {
        self.inner.stat(ctx, path, args).await
    }
    fn read(&self, ctx: &OperationContext, path: &str, args: OpRead) -> file::Result<Self::Reader> {
        self.inner.read(ctx, path, args)
    }
    fn write(
        &self,
        ctx: &OperationContext,
        path: &str,
        args: OpWrite,
    ) -> file::Result<Self::Writer> {
        self.inner.write(ctx, path, args)
    }
    fn delete(&self, ctx: &OperationContext) -> file::Result<Self::Deleter> {
        self.inner.delete(ctx)
    }
    fn list(&self, ctx: &OperationContext, path: &str, args: OpList) -> file::Result<Self::Lister> {
        self.inner.list(ctx, path, args)
    }
    fn copy(
        &self,
        ctx: &OperationContext,
        from: &str,
        to: &str,
        args: OpCopy,
    ) -> file::Result<Self::Copier> {
        self.inner.copy(ctx, from, to, args)
    }
    async fn rename(
        &self,
        ctx: &OperationContext,
        from: &str,
        to: &str,
        args: OpRename,
    ) -> file::Result<RpRename> {
        self.inner.rename(ctx, from, to, args).await
    }
    async fn presign(
        &self,
        ctx: &OperationContext,
        path: &str,
        args: OpPresign,
    ) -> file::Result<RpPresign> {
        self.probe.presigns.fetch_add(1, Ordering::SeqCst);
        if let Some(kind) = *self.probe.failure.lock().unwrap() {
            return Err(file::Error::new(kind, "injected presign failure"));
        }
        let signed = self
            .inner
            .presign(ctx, path, args)
            .await?
            .into_presigned_request();
        if self.probe.require_header.load(Ordering::SeqCst) {
            let mut headers = signed.header().clone();
            headers.insert("x-required", http::HeaderValue::from_static("test"));
            return Ok(RpPresign::new(PresignedRequest::new(
                signed.method().clone(),
                signed.uri().clone(),
                headers,
            )));
        }
        Ok(RpPresign::new(signed))
    }
}
pub struct Fixture {
    pub store: S3AssetResolver,
    pub transport: Transport,
    pub probe: Arc<Probe>,
}
pub fn config() -> S3AssetConfig {
    let mut config = S3AssetConfig::new(
        "https://account.r2.cloudflarestorage.com",
        "assets",
        "auto",
        "test-access",
        "test-secret",
    );
    config.key_prefix = "/images/".into();
    config.presign_expiry = std::time::Duration::from_secs(90);
    config
}
pub fn fixture(config: S3AssetConfig) -> Fixture {
    let transport = Transport::default();
    let probe = Arc::new(Probe::default());
    let operator =
        file::s3_with_transport(config.builder(), HttpTransporter::new(transport.clone()))
            .unwrap()
            .layer(CountLayer(probe.clone()));
    let store = S3AssetResolver::from_operator(operator, config.key_prefix, config.presign_expiry);
    Fixture {
        store,
        transport,
        probe,
    }
}
