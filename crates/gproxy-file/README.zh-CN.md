# gproxy-file

[English](README.md) | 简体中文

GPROXY 的可选文件内容库，复用 Apache OpenDAL 的本地文件和 S3 兼容后端。
Cloudflare R2 使用 S3 后端。统一读写接口为本 crate 重新导出的 OpenDAL `Operator`。

文件归属、业务元数据、保留策略和上游资源 ID 映射由调用方负责。
本库不依赖 SeaORM 或 gproxy core。

## Features

默认不启用 `fs` 或 `s3`。

| Feature | 后端 | 原生 | Workers WASM |
|---|---|---|---|
| `fs` | 本地文件系统 | 支持 | 不支持 |
| `s3` | S3 兼容对象存储，包括 R2 | 支持 | 支持 |

宿主可以把整个库声明为可选依赖：

```toml
[dependencies]
gproxy-file = { version = "4.0.0", optional = true, default-features = false, features = ["fs", "s3"] }
```

## 本地文件

启用 `fs`。原生文件操作在 Tokio runtime 中执行。

```rust
let files = gproxy_file::filesystem("/srv/gproxy/files")?;
files.write("example.txt", "hello").await?;
let content = files.read("example.txt").await?.to_bytes();
let metadata = files.stat("example.txt").await?;
files.delete("example.txt").await?;
```

也可通过重新导出的 `Fs`、`FsConfig` 直接配置 OpenDAL。

## S3 和 Cloudflare R2

启用 `s3`，通过标准 OpenDAL builder 传入 bucket、endpoint、region 和凭证。
R2 的 region 使用 `auto`：

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

AWS S3 使用 bucket 所在 region 和对应 endpoint。认证、签名、对象路径、multipart 操作
及错误处理沿用 OpenDAL。默认 HTTP transport 在原生使用 reqwest + rustls，在 WASM
使用宿主 Fetch API。并发 multipart 任务在原生使用 Tokio，在 Workers 使用 WASM
microtask 执行器。

宿主可以注入自己的 OpenDAL HTTP transport：

```rust
use gproxy_file::{opendal::HttpTransporter, s3_with_transport};

let files = s3_with_transport(builder, HttpTransporter::new(my_transport))?;
```

## 流式读写及其他操作

reader、writer、范围读取、列表、元信息和删除均使用 OpenDAL 的 `Operator` API，
各后端的能力保持 OpenDAL 原有语义。

```rust
let mut writer = files.writer("example.bin").await?;
writer.write("first part").await?;
writer.write("second part").await?;
writer.close().await?;

let part = files.read_with("example.bin").range(0..5).await?;
```

其他 OpenDAL 类型可通过 `gproxy_file::opendal` 使用。

## 验证

```bash
cargo test -p gproxy-file --features fs,s3
cargo check -p gproxy-file --no-default-features
cargo check -p gproxy-file --no-default-features --features s3 --target wasm32-unknown-unknown
```

测试实际读写临时本地文件，覆盖流式写入和范围读取；S3 测试通过注入的测试 transport
验证签名、路径编码、读写、元信息和删除。另行检查 WASM 编译；这些测试不连接真实
S3/R2 账户。
