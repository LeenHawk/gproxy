//! Optional file-content storage using OpenDAL's filesystem and S3 services.
//!
//! The common API is [`Operator`]: read/write, streaming readers/writers, stat,
//! list and delete. Backend behavior and errors come from OpenDAL. Application
//! entities, ownership, retention and resource-ID mappings stay with the caller.

pub use opendal_core as opendal;
pub use opendal_core::{Buffer, Error, ErrorKind, Metadata, Operator, Result};

#[cfg(all(feature = "fs", not(target_arch = "wasm32")))]
pub use opendal_service_fs::{Fs, FsConfig};
#[cfg(feature = "s3")]
pub use opendal_service_s3::{S3, S3Config};

/// Open a local filesystem root using OpenDAL's filesystem service.
#[cfg(all(feature = "fs", not(target_arch = "wasm32")))]
pub fn filesystem(root: &str) -> Result<Operator> {
    Operator::new(Fs::default().root(root))
}

/// Open S3-compatible storage with the default reqwest transport.
/// Native targets use rustls; WASM uses the host Fetch API.
#[cfg(feature = "s3")]
pub fn s3(builder: S3) -> Result<Operator> {
    s3_with_transport(
        builder,
        opendal::HttpTransporter::new(opendal_http_transport_reqwest::ReqwestTransport::default()),
    )
}

/// Open S3-compatible storage with a caller-supplied HTTP transport.
/// This lets a host reuse its client configuration without a global override.
#[cfg(feature = "s3")]
pub fn s3_with_transport(builder: S3, transport: opendal::HttpTransporter) -> Result<Operator> {
    let context = opendal::OperationContext::new().with_http_transport(transport);
    #[cfg(target_arch = "wasm32")]
    let context = context.with_executor(opendal::Executor::with(WasmExecutor));
    Ok(Operator::new(builder)?.with_context(context))
}

// OpenDAL needs an executor for concurrent multipart work. Workers do not run
// Tokio; use the WASM microtask executor for the same OpenDAL task lifecycle.
#[cfg(all(feature = "s3", target_arch = "wasm32"))]
#[derive(Debug)]
struct WasmExecutor;

#[cfg(all(feature = "s3", target_arch = "wasm32"))]
impl opendal::Execute for WasmExecutor {
    fn execute(&self, task: opendal::raw::BoxedStaticFuture<()>) {
        wasm_bindgen_futures::spawn_local(task);
    }
}
