---
title: 安装
description: 如何从源码、发布二进制或 Docker 镜像安装 gproxy。
---

gproxy 以单一静态二进制的形式发布，React 控制台已经内嵌在里面。获取方式有三种：

1. **从源码编译** (开发推荐)。
2. **下载发布二进制** (GitHub Releases 页面)。
3. **运行 Docker 镜像** (基于仓库中的 [`Dockerfile.action`](https://github.com/LeenHawk/gproxy/blob/main/Dockerfile.action))。

## 前置要求

- **Rust 1.80+** (edition 2024，如存在 `rust-toolchain`，以其为准)。
- **pnpm 9+** 和 **Node 20+** —— 仅在你需要重新构建内嵌控制台时需要。
- 受支持的数据库 —— 默认 **SQLite** 即可工作；如有需要可换成 **PostgreSQL** 或 **MySQL**。

## 从源码编译

克隆仓库并做 release 构建：

```bash
git clone https://github.com/LeenHawk/gproxy.git
cd gproxy
cargo build -p gproxy --release
```

生成的二进制位于 `target/release/gproxy`。

### 重新构建内嵌控制台

如果你改动了 `frontend/console/` 下的任何内容，先重新构建控制台再构建二进制，
新资源才会被打包进去：

```bash
cd frontend/console
pnpm install
pnpm build
cd ../..
cargo build -p gproxy --release
```

`pnpm build` 会把产物写到 server crate 通过 `include_dir!` 引用的目录，
不需要单独部署静态文件。

## 发布二进制

每次打 tag 时，GitHub Releases 会发布预构建的二进制。下载对应平台的压缩包，
解压出 `gproxy`，赋予执行权限后直接运行：

```bash
chmod +x gproxy
./gproxy
```

## Docker

仓库中提供参考 Dockerfile [`Dockerfile.action`](https://github.com/LeenHawk/gproxy/blob/main/Dockerfile.action)。
完整的容器部署示例 (挂载数据目录、传入环境变量等) 见
[Docker 部署](/zh-cn/deployment/docker/)。

## 下一步

- 按照 [快速开始](/zh-cn/getting-started/quick-start/) 用一个供应商和一个用户
  把实例跑起来。
- 浏览 [环境变量参考](/zh-cn/reference/environment-variables/) 了解可调项。
