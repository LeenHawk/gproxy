# gproxy-file

English | [简体中文](README.zh-CN.md)

Optional file-content storage for GPROXY, using Apache OpenDAL's local filesystem
and S3-compatible services. Cloudflare R2 uses the S3 backend. The common API is
OpenDAL's `Operator`, re-exported by this crate.

Application ownership, file metadata records, retention, and upstream resource-ID
mappings stay with the caller. This crate does not depend on SeaORM or gproxy core.

## Features

Neither `fs` nor `s3` is enabled by default.

| Feature | Backend | Native | Workers WASM |
|---|---|---|---|
| `fs` | Local filesystem | Yes | No |
| `s3` | S3-compatible object storage, including R2 | Yes | Yes |

An embedding crate can make the entire dependency optional:

```toml
[dependencies]
gproxy-file = { version = "4.0.0-dev", optional = true, default-features = false, features = ["fs", "s3"] }
```

## Local files

Enable `fs`. File operations run inside a Tokio runtime on native targets.

```rust
let files = gproxy_file::filesystem("/srv/gproxy/files")?;
files.write("example.txt", "hello").await?;
let content = files.read("example.txt").await?.to_bytes();
let metadata = files.stat("example.txt").await?;
files.delete("example.txt").await?;
```

`Fs` and `FsConfig` are also re-exported for configuring OpenDAL directly.

## S3 and Cloudflare R2

Enable `s3`. Pass the bucket, endpoint, region, and credentials to the standard
OpenDAL builder. For R2, the region is `auto`:

```rust
use gproxy_file::{S3, s3};

let files = s3(
    S3::default()
        .bucket("my-bucket")
        .endpoint("https://<account-id>.r2.cloudflarestorage.com")
        .region("auto")
        .access_key_id(&access_key_id)
        .secret_access_key(&secret_access_key),
)?;

files.write("example.txt", "hello").await?;
```

For AWS S3, use the bucket's AWS region and the appropriate endpoint. Authentication,
request signing, object paths, multipart operations, and errors follow OpenDAL.
The default HTTP transport uses reqwest with rustls on native targets and the host
Fetch API on WASM. Concurrent multipart tasks use Tokio on native targets and
the WASM microtask executor on Workers.

Hosts can inject their own OpenDAL HTTP transport:

```rust
use gproxy_file::{opendal::HttpTransporter, s3_with_transport};

let files = s3_with_transport(builder, HttpTransporter::new(my_transport))?;
```

## Streaming and other operations

Use the normal OpenDAL `Operator` API for readers, writers, ranges, listing,
metadata, and deletion. Backend capabilities remain those of OpenDAL.

```rust
let mut writer = files.writer("example.bin").await?;
writer.write("first part").await?;
writer.write("second part").await?;
writer.close().await?;

let part = files.read_with("example.bin").range(0..5).await?;
```

Advanced OpenDAL types are available through `gproxy_file::opendal`.

## Validation

```bash
cargo test -p gproxy-file --features fs,s3
cargo check -p gproxy-file --no-default-features
cargo check -p gproxy-file --no-default-features --features s3 --target wasm32-unknown-unknown
```

Tests exercise actual temporary local files, streaming writes and range reads,
plus S3 signing, encoded paths, reads, writes, metadata, and deletion through an
injected test transport. WASM compilation is checked separately; these tests do
not contact a live S3/R2 account.
