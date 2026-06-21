---
title: 安装
description: 通过 release 二进制、Docker 镜像、源码构建或 edge bundle 安装 GPROXY v2。
---

GPROXY v2 是一个单 Rust crate，native 产物是名为 `gproxy` 的二进制。同一个 crate
也可以编译成 edge WebAssembly runtime。native 形态下，React Console 不是独立服务：
构建 `console/` 后，静态文件会同步到 `assets/console/`，再被编进二进制。

按部署形态选择安装方式。

## Release 二进制

如果只想运行 native server 和内嵌 Console，不想在机器上安装 Rust 或 Node，使用
release 二进制。

1. 从 GitHub release 下载对应 OS 和 CPU 的压缩包。
2. 解压 `gproxy` 或 `gproxy.exe`。
3. 放到 `PATH` 中，或直接运行。

```bash
chmod +x ./gproxy
./gproxy --help
```

release workflow 会构建 Linux、macOS、Windows、Android，以及 x86_64、aarch64
目标。Docker 镜像也使用预构建的 Linux 二进制作为输入。

## Docker 镜像

发布镜像是 `ghcr.io/leenhawk/gproxy`。

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
docker run --rm -p 8787:8787 \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

镜像里已经包含带内嵌 Console 的 native 二进制。镜像默认设置
`GPROXY_HOST=0.0.0.0`、`GPROXY_PORT=8787`、`GPROXY_PERSISTENCE=file`、
`GPROXY_DATA_DIR=/app/data`。

持久化 volume、PostgreSQL/MySQL DSN 和 tag 选择见 [Docker](/zh-cn/deployment/docker/)。

## 从源码构建

开发 GPROXY，或 release 尚未包含目标平台时，使用源码构建。

前置条件：

- 支持 edition 2024 的当前 stable Rust 工具链。
- 如果要嵌入当前 `console/` 代码，需要 Node.js 和 pnpm；release workflow 使用
  Node 22 和 pnpm 9。
- 目标平台所需的系统库。

需要嵌入 Console 时，先构建前端：

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..
```

再从仓库根目录构建二进制：

```bash
cargo build --release --bin gproxy
./target/release/gproxy --help
```

`pnpm build` 会生成 `console/dist/`，再运行
`console/scripts/sync-to-embed.mjs` 同步到 `assets/console/`。native 二进制通过
`rust-embed` 编译这个目录。

如果跳过 Console 构建，gateway 和 admin API 仍可编译运行，但 `/console` 可能返回
`console assets not embedded`。

## Edge Bundles

不要让 edge 平台从源码编译 Rust。支持的 edge 路径是上传预构建 bundle：

```text
在有 Rust 的机器/CI 构建 wasm -> 生成平台 bundle -> 上传 bundle
```

release artifacts 包含 `gproxy-edge-cloudflare.zip`、`gproxy-edge-netlify.zip`、
`gproxy-edge-supabase.zip`、`gproxy-edge-deno.zip`、`gproxy-edge-eopages.zip` 和
`gproxy-edge-appwrite-deno.zip`。

平台命令和 runtime secrets 见 [Edge Wasm 部署](/zh-cn/deployment/edge/)。

## 下一步

- 继续 [快速开始](/zh-cn/getting-started/quick-start/)，启动本地实例。
- 反代 native server 前先读 [内嵌 Console](/zh-cn/guides/console/)。
- 把 v2 指向已有 v1 数据目录前，先读 [从 v1 迁移到 v2](/zh-cn/deployment/v1-to-v2/)。
