---
title: "从源码构建"
description: 从源码构建 GPROXY v4 的每个目标——server、桌面宿主、Worker 与 console——并运行 CI 跑的那几道质量闸门。
---

`dev` 分支开发新功能，推送后只更新 `nightly`（dev 更新通道）。`main` 分支维护已发布功能和修复 bug，推送后只更新 `staging`（beta 更新通道）。
正式版本从 `main` 发布，同时更新 release 和 beta，不覆盖独立开发的 dev 通道。版本 tag 必须与 workspace 版本一致。
推送 dev 前执行 `bash scripts/push-dev.sh`，先 rebase 到最新 main；使用 `git config core.hooksPath .githooks` 启用本地推送检查。
自动打包由 `.github/workflows/release.yml` 驱动。

## 发布包

CLI（`gproxy-*`）和 Application（`gproxy-tauri-*`）是两类独立程序，分别提供
便携包和安装包。Application 内嵌构建好的 Console。

| 平台 | CLI | Application |
| --- | --- | --- |
| Linux GNU（x86_64、aarch64、riscv64） | ZIP、DEB | ZIP、DEB |
| Linux musl（x86_64、aarch64、riscv64） | ZIP、DEB | — |
| Windows（x86_64、aarch64） | ZIP、MSIX | ZIP、MSIX |
| macOS（x86_64、aarch64） | ZIP、DMG | ZIP、DMG |
| Android（x86_64、aarch64） | ZIP、Termux DEB | 仅 APK |
| OpenHarmony / HarmonyOS NEXT | ARM64 / x86_64 ZIP | ARM64 实验性未签名 HAP |

Linux CLI DEB 将 `gproxy` 安装到 `/usr/bin`；Termux DEB 安装到
`/data/data/com.termux/files/usr`。Android CLI 使用 `distribution/termux/` 中的源码配方
和固定版本的 Termux 官方构建器。DEB 和 ZIP 都依赖 Termux 的 `libc++`、OpenSSL 和
CA 证书；ZIP 包含启动脚本、二进制及安装更新说明 `TERMUX.txt`。
这些构建禁用程序自更新，通过 APT 安装新版 DEB；启用的软件仓库收录后可使用
`pkg upgrade gproxy`。Windows CLI MSIX 使用独立的
`.CLI` 包身份和控制台命令别名；macOS CLI DMG 包含二进制及终端安装说明。
Application ZIP 保留桌面资源，macOS ZIP 包含完整 `.app`。

鸿蒙构建使用缓存工具链镜像和固定版本的实验性 Tauri 分支，其他平台继续使用稳定版。
HAP 安装前需要签名，尚未做真机验证；暂不提供后台服务。

nightly 附件使用固定名称，提交 SHA 记录在更新清单中。Linux x86_64 在 Ubuntu 22.04 构建，
ARM64 在 Ubuntu 24.04 构建，安装时需要发行版提供 WebKitGTK 4.1。
macOS 使用 ad-hoc 签名，尚未接入 Developer ID 签名和公证。

Windows 使用 Microsoft Store 的包身份。`release` 环境需配置四个变量：
`MS_STORE_IDENTITY_NAME`、`MS_STORE_DISPLAY_NAME`、`MS_STORE_IDENTITY_PUBLISHER`、
`MS_STORE_PUBLISHER_DISPLAY_NAME`。正式版保留两架构的 Store 提交包；启用
`MS_STORE_PUBLISH_ENABLED` 后，GitHub Release 发布成功会触发 Store 提交流程。
GitHub 附带的 MSIX 与提交包一样未签名，由 Store 签发后分发；它不是可直接双击安装的
可信签名包。应用依赖系统的 WebView2 Runtime。

Android 使用已有 `ANDROID_SIGNING_*` secrets 签名并验证 APK。应用的
`<target-triple>-tauri-apk` 条目进入 Ed25519 签名更新清单，与 CLI ZIP 更新分开；不再构建旧服务包装 APK。缺少必需的密钥或 Store 身份会使打包失败。

本地先构建 Console 并同步资源，然后调用发布脚本：

```sh
pnpm --dir console build
node console/scripts/sync-to-embed.mjs
pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
TARGET_OS=linux TARGET_TRIPLE=x86_64-unknown-linux-gnu \
  ARTIFACT_NAME=gproxy-tauri-linux-x86_64 \
  GPROXY_BUILD_VERSION=$(scripts/release-metadata.sh version) \
  scripts/package-tauri-release.sh
```

产物写入 `dist/release/`。工作流还发布原生服务器 ZIP、Termux 包、Edge 包和
GNU/musl 容器镜像；每个应用包都生成构建证明，GitHub 上可查询 attestations。

## 构建环境

需要 stable Rust（edition 2024）、Go、Clang、Node.js 22.12+（推荐 24 LTS）和 pnpm。Linux 桌面构建还需要 `webkit2gtk-4.1`、`gtk+-3.0`、`libsoup-3.0` 开发包。其他平台工具链由发布工作流配置。

## 构建 CLI 与控制台

从仓库根目录运行：

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build
cargo build -p gproxy --release
./target/release/gproxy serve --data-dir ./data
```

控制台构建会自动同步资源到 HTTP 宿主和 Tauri 的嵌入目录，随后编译的二进制包含控制台。只执行 Rust 构建的全新检出没有这些资源，`/console` 会返回 404。也可以使用 `--console-path console/dist` 指向单独构建的资源。

CLI 默认启用全部渠道、内存缓存、本地文件存储和内置词表。可按需缩减渠道或增加后端：

```sh
cargo build -p gproxy --release --no-default-features \
  --features memory,fs,codex,claudecode,openai,custom
```

PostgreSQL、MySQL、Redis 和 S3 分别需要 `postgres`、`mysql`、`redis`、`s3` feature。SQLite 始终可用。

## 构建 Application

构建控制台后运行：

```sh
cargo run -p gproxy-host-tauri --bin gproxy-desktop
```

全新实例会打开设置向导。窗口通过 IPC 管理实例，HTTP 监听供外部客户端调用网关。移动端打包请使用发布工作流中的平台工具链与打包脚本。

默认 workspace 成员不包含 Tauri，便于没有桌面依赖的环境编译后端；`--workspace` 会包含它。

## 构建 Workers

从仓库根目录运行：

```sh
rustup target add wasm32-unknown-unknown
cargo install worker-build
pnpm --dir console install --frozen-lockfile
pnpm --dir deploy/cloudflare install
pnpm --dir deploy/cloudflare build
pnpm --dir deploy/cloudflare check
```

`worker-build` 完成 WASM 编译与优化；`check` 使用 Wrangler 做部署预检，不发布到线上。配置和运行限制见[边缘部署](/zh-cn/deployment/edge/)。

## 检查改动

后端默认成员：

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

桌面宿主单独检查：

```sh
cargo clippy -p gproxy-host-tauri --all-targets -- -D warnings
cargo test -p gproxy-host-tauri
```

WASM 检查只包含支持该目标的库和宿主，不能对包含 CLI、Tauri 的整个 workspace 执行：

```sh
cargo check --target wasm32-unknown-unknown \
  -p gproxy-protocol -p gproxy-store -p gproxy-seaorm -p gproxy-client \
  -p gproxy-cache -p gproxy-channel -p gproxy-core -p gproxy-file \
  -p gproxy-tokenizer -p gproxy-sdk -p gproxy-app -p gproxy-host-axum \
  -p gproxy-host-edge
```

控制台与文档：

```sh
pnpm --dir console lint
pnpm --dir console test
pnpm --dir console build
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs check
pnpm --dir docs build
bash scripts/check-docs.sh
```

修改 Rust DTO 后，用 `pnpm --dir console types` 更新生成的 TypeScript 类型。

## 发布

版本 tag 必须与 workspace 版本一致，并提供 `docs/release-notes/v<版本>.md`。发布流程构建包、生成签名更新清单并上传附件；构建或签名要求未满足时，不能视为发布完成。

版本 tag 还会调用 `scripts/publish-crates.sh` 发布选定的 MIT 库，其他 crate 使用 git 或路径依赖。详情见[嵌入核心库](/zh-cn/reference/embedding/)。
