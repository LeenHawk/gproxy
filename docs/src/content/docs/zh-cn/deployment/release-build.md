---
title: "构建与发布"
description: "从源码构建 gproxy 与 edge wasm，运行质量门禁，并了解由 tag 驱动、对每个产物签名并发布的流水线"
---

GPROXY 以一个原生二进制 `gproxy` 交付，构建自 `crates/gproxy-host-axum`；另有
一个面向 fetch 平台的 wasm host，`crates/gproxy-host-edge`。操作员控制台只编译
一次并内嵌进原生二进制，因此源码构建总是从控制台开始。

## 前置条件

| 工具 | 用途 |
| --- | --- |
| Rust stable 工具链（edition 2024） | 全部 crate；构建 edge host 需额外添加 `wasm32-unknown-unknown` target |
| Node.js LTS 与 pnpm 9 | 控制台（`console/`）与文档（`docs/`） |
| 与 `Cargo.lock` 版本一致的 `wasm-bindgen-cli`，或 `wasm-pack` | 生成 edge glue |
| 带 buildx 的 Docker | 容器镜像 |
| `cross`、`cargo-ndk`、WiX 4、`dpkg-deb`、`hdiutil` | 仅 release 打包需要 |

## 构建控制台

```sh
cd console
pnpm install --frozen-lockfile
pnpm build
```

`pnpm build` 依次执行 `tsc -b`、`vite build` 和 `scripts/sync-to-embed.mjs`，
最后一步把 `console/dist/` 复制到 `crates/gproxy-host-axum/assets/web/`。该目录
除 `.gitkeep` 外均被 gitignore；原生 host 在编译期通过 `rust-embed` 嵌入它。跳过
这一步时二进制仍能提供 API，但 `/`、`/admin` 和 `/portal` 会返回 `404`，正文为
`web assets are not embedded; run pnpm build in console/ and rebuild gproxy`。

## 构建原生二进制

```sh
cargo build --release -p gproxy-host-axum
./target/release/gproxy --version
```

产物是 `target/release/gproxy`。`cargo run -p gproxy-host-axum` 以默认值
（`127.0.0.1:8787`、`./data`、SQLite）运行 debug 构建。`--version` 打印构建标识：

```text
gproxy 3.0.0 (channel development, build 4054fe4f94ea, installation source)
```

构建标识在编译期由以下变量固定，`crates/gproxy-host-axum/src/lib.rs` 用
`option_env!` 读取：

| 变量 | 默认值 | Release 取值 |
| --- | --- | --- |
| `GPROXY_BUILD_VERSION` | Cargo package 版本 | workspace 版本 |
| `GPROXY_BUILD_CHANNEL` | `development` | `releases` 或 `dev` |
| `GPROXY_BUILD_HASH` | `build.rs` 取到的 git 短哈希，否则 `unknown` | 提交 SHA |
| `GPROXY_INSTALLATION_KIND` | `source` | `standalone`、`container` 或 `android-apk` |
| `GPROXY_UPDATE_PUBKEY` | 未设置 | base64 的 Ed25519 公钥，32 字节 |

没有 `GPROXY_UPDATE_PUBKEY` 时，二进制没有可用于校验更新 manifest 和公告 feed 的
公钥，两项校验都会失败。开发构建不需要它。

## 构建 Edge Wasm

```sh
rustup target add wasm32-unknown-unknown
cargo build -p gproxy-host-edge --release --target wasm32-unknown-unknown
wasm-bindgen --target bundler --out-dir deploy/cloudflare/pkg \
  --out-name gproxy_host_edge \
  target/wasm32-unknown-unknown/release/gproxy_host_edge.wasm
```

Cloudflare 使用 `bundler` target；Deno 与 Netlify 使用 `--target web`。
`wasm-bindgen` CLI 的版本必须等于 `Cargo.lock` 中 `wasm-bindgen` crate 的版本。
`scripts/package-edge-release.sh` 会完成构建、生成两种 glue、把预构建的
`console/dist` 复制到各 `deploy/<platform>/public/`，并打包三个 bundle 的 zip。
各平台目录还带有 `pnpm run build` / `deno task build` 脚本，通过 `wasm-pack`
完成同样的工作；见 [Edge Wasm](/zh-cn/deployment/edge/)。

## 质量门禁

后端与控制台变更收尾时运行与 CI 相同的命令：

| 命令 | 检查内容 |
| --- | --- |
| `cargo fmt --check` | 格式 |
| `cargo clippy --workspace --all-targets -- -D warnings` | lint；任何 warning 都会失败 |
| `cargo test --workspace` | 测试；同时重新生成 TypeScript DTO |
| `cargo check --workspace --target wasm32-unknown-unknown` | 核心仍可为 edge 编译 |
| `pnpm lint`（在 `console/` 中） | `tsc -b`、ESLint、多语言一致性（`pnpm i18n:check`） |
| `pnpm test`（在 `console/` 中） | Vitest 与模型目录脚本测试 |
| `pnpm build`（在 `console/` 中） | 生产 bundle |

管理 API 的 DTO 派生 `ts_rs::TS`；`cargo test` 把它们写入 `console/src/generated/`。
这些文件是生成产物：修改 Rust 类型，运行 `cargo test`，提交结果。不要手工编辑。

## CI

`.github/workflows/ci.yml` 在每次 push 和 pull request 时运行三个 job：
**Backend**（上述四个 cargo 门禁）、**Console**（`pnpm install --frozen-lockfile`、
lint、test、build、`i18n:check`）和 **Docs**（在 `docs/` 中执行 `pnpm check`、
`pnpm build`）。推送到默认分支或 `3.0` 时还会运行 **Deploy docs**：用更新签名
密钥对 `notifications.json` 签名（生成 `notifications.json.sig`），并把站点发布到
Cloudflare Pages。原生二进制轮询该 feed，并用同一把编译进二进制的公钥校验。

## 发起一次发布

```sh
scripts/release.sh
```

脚本从 `Cargo.toml` 读取 `[workspace.package].version`，要求已跟踪的工作区干净，
若 tag `v<version>` 不存在则创建带注释的 tag，若 tag 指向其他提交则拒绝，并且只
推送这个 tag。对同一提交重复运行是安全的。带预发布后缀的版本（`3.0.0-alpha.0`）
构建 `dev` channel；普通版本构建 `releases`。

## 发布流水线

推送 tag 会触发 `.github/workflows/release.yml`。job 按顺序为：

1. **Release metadata** —— 校验 tag 等于 `v<workspace version>`，推导 channel，
   并从 `scripts/release-targets.json` 载入 target 矩阵。
2. **Console and container image** —— 通过 `deploy/container/Dockerfile` 的
   `console-dist` 阶段只构建一次控制台，构建并推送
   `ghcr.io/leenhawk/gproxy:<tag>`（linux/amd64，带
   BuildKit provenance 与 SBOM attestation），并把同一镜像保存为
   `gproxy-container-linux-amd64.tar.gz`。控制台产物作为 workflow artifact 传给
   后续 job。
3. **Native `<target>`** —— 矩阵每行一个 job。各自把控制台下载到
   `crates/gproxy-host-axum/assets/web`，检查更新公钥能解码为 32 字节，用
   `cargo`、`cross` 或 `cargo-ndk`（Android API 28）构建 `--bin gproxy`，然后
   打包。Windows 构建设置 `RUSTFLAGS=-C target-feature=+crt-static`；macOS 二进制
   用 `codesign --sign -` 做 ad hoc 签名。
4. **Signed update manifest** —— `scripts/build-update-manifest.sh` 收集每个原生
   zip 和 Android APK，为每个记录 `target_triple`、`url`、`sha256` 和 `size`，从
   `crates/gproxy-store/src/schema/catalog.rs` 中的 `Control` schema 版本推导
   `min_compatible_data_version`，并用 Ed25519 私钥对规范化 payload 签名。私钥与
   公钥不匹配时中止。
5. **Edge bundles** —— 安装匹配的 `wasm-bindgen-cli`，运行
   `scripts/package-edge-release.sh`，并对三个平台入口做类型检查（`pnpm check`、
   `deno check`）。
6. **Publish release** —— 创建或更新 GitHub release `v<version>`（`dev` 构建加
   `--prerelease`）并上传全部文件。对 `dev` 构建还会把 `dev` tag 强制移到该提交，
   并把 `manifest.json` 上传到名为 `dev` 的固定预发布版本，除非已存在更新的 v3
   预发布版本。

### 原生 Target

| 产物 | Target triple | 构建器 | 安装包 |
| --- | --- | --- | --- |
| `gproxy-linux-x86_64` | `x86_64-unknown-linux-gnu` | cargo | `.deb` |
| `gproxy-linux-aarch64` | `aarch64-unknown-linux-gnu` | cargo（arm runner） | `.deb` |
| `gproxy-linux-riscv64` | `riscv64gc-unknown-linux-gnu` | cross | `.deb` |
| `gproxy-linux-x86_64-musl` | `x86_64-unknown-linux-musl` | cross | `.deb` |
| `gproxy-linux-aarch64-musl` | `aarch64-unknown-linux-musl` | cross | `.deb` |
| `gproxy-linux-riscv64-musl` | `riscv64gc-unknown-linux-musl` | cross | `.deb` |
| `gproxy-macos-x86_64` | `x86_64-apple-darwin` | cargo | `.dmg` |
| `gproxy-macos-aarch64` | `aarch64-apple-darwin` | cargo | `.dmg` |
| `gproxy-windows-x86_64` | `x86_64-pc-windows-msvc` | cargo | `.msi` |
| `gproxy-windows-aarch64` | `aarch64-pc-windows-msvc` | cargo | `.msi` |
| `gproxy-android-x86_64` | `x86_64-linux-android` | cargo-ndk | `.apk` |
| `gproxy-android-aarch64` | `aarch64-linux-android` | cargo-ndk | `.apk` |

每个产物都有一个 `.zip`（二进制、`README.md`、`LICENSE`）、表中列出的安装包、每个
文件旁的 `.sha256`，以及一个 `.provenance.json`。Android zip 中 ELF 名为
`gproxy.bin`，附带 NDK 的 `libc++_shared.so` 和一个 `gproxy` 启动脚本；APK 封装
同一份 payload。release 还包含 `manifest.json`、`gproxy-edge.wasm`、
`gproxy-edge-{cloudflare,deno,netlify}.zip` 和 `gproxy-container-linux-amd64.tar.gz`，
均带校验和。确切名称见 `scripts/release-targets.json` 与 workflow。

## 签名

| 机制 | 签名对象 | CI secret |
| --- | --- | --- |
| macOS ad hoc `codesign --sign -` | 二进制与 DMG 内的 `.app` | 无 |
| Android `apksigner` | 每个 `.apk` | `ANDROID_SIGNING_KEYSTORE_B64`、`ANDROID_SIGNING_KEYSTORE_PASSWORD`、`ANDROID_SIGNING_KEY_ALIAS`，可选 `ANDROID_SIGNING_KEY_PASSWORD` |
| Ed25519 更新密钥 | `manifest.json` 与 `notifications.json` | `UPDATE_SIGNING_PRIVATE_KEY_B64`（base64 的 PEM）、`UPDATE_SIGNING_PUBLIC_KEY_B64`（base64 的原始公钥） |

公钥一半以 `GPROXY_UPDATE_PUBKEY` 编译进每个二进制，因此二进制只接受由对应私钥
签名的 manifest 与公告。按脚本期待的形式生成密钥对：

```sh
openssl genpkey -algorithm ed25519 -out update.pem
base64 -w0 update.pem                                    # UPDATE_SIGNING_PRIVATE_KEY_B64
openssl pkey -in update.pem -pubout -outform DER \
  | tail -c 32 | base64 -w0                              # UPDATE_SIGNING_PUBLIC_KEY_B64
```

文档部署另外需要 `CLOUDFLARE_API_TOKEN`、`CLOUDFLARE_ACCOUNT_ID` 和
`CLOUDFLARE_PROJECT_ID`。

## 构建溯源

`scripts/build-provenance.sh` 为每个产物写一个 `<artifact>.provenance.json`：
`version`、`commit`、`tag`、`target`、`builder`，实际运行的 `rustc`、`node`、`pnpm`
版本，以及 `deploy/container/Dockerfile` 中每一行 `FROM` 的镜像引用和构建时解析到
的 digest。镜像 tag 在固定的版本线内会漂移；这份记录才是日后识别一次构建的依据。

## 更新 Channel

已发布的二进制根据签名 manifest 检查更新：

| Channel | Manifest URL |
| --- | --- |
| `releases` | `https://github.com/LeenHawk/gproxy/releases/latest/download/manifest.json` |
| `staging` | `https://github.com/LeenHawk/gproxy/releases/download/staging/manifest.json` |
| `dev` | `https://github.com/LeenHawk/gproxy/releases/download/dev/manifest.json` |

编译进去的 channel 是默认值；控制台的更新 channel 设置或 `GPROXY_UPDATE_CHANNEL`
可覆盖它，`GPROXY_UPDATE_SERVE` 可指向自托管的 manifest。GitHub 的
`releases/latest` 永远不会解析到预发布版本，因此预发布构建以 `dev` channel 编译，
并跟随 `dev` manifest。其余仅原生可用的变量见[配置](/zh-cn/reference/configuration/)。
