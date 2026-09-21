---
title: "从源码构建"
description: 从源码构建 GPROXY v4 的每个目标——server、桌面宿主、Worker 与 console——并运行 CI 跑的那几道质量闸门。
---

从源码构建是拿到 v4 的唯一方式。**没有发布流水线**：没有安装包、没有便携包、没有发布的
容器镜像、没有签名的更新清单，也没有代码签名。

这是关于 v4 当前状态的陈述，不是一条方针。v3 这些全都有，而一样都没有被移植；描述它们的
那些页面被删除，而不是围绕不存在的机制改写。仓库里的 `deploy/` 与 `scripts/` 目录仍然装着
v3 的流水线，而且**构建不了 v4**——比如那份容器 Dockerfile 要的二进制目标已经换了 crate。

## 前置条件

| 工具 | 用于 |
| --- | --- |
| stable Rust 工具链（edition 2024） | 每个 crate |
| `wasm32-unknown-unknown` | Workers 宿主，以及 CI 的那一道检查 |
| `webkit2gtk-4.1`、`gtk+-3.0`、`libsoup-3.0`（Linux） | 桌面宿主 |
| Node.js LTS 与 pnpm | console 与本文档站 |
| `worker-build` | Workers 包 |

## Workspace

十六个 crate。不带 `-p` 也不带 `--workspace` 的 `cargo check`、`cargo test` 和
`cargo clippy` 构建的是**默认成员**，也就是除桌面宿主之外的一切。

这个排除不是降级，也不是为了增量构建：目标目录是热的时候，两种选择相差不到半秒。它是为了
**冷**的那一次。桌面宿主会拉进 wry、webkit、gtk 及它们的 `-sys` crate——大约一百八十个与
引擎无关的第三方 crate，而且一台没有对应开发头文件的 Linux 机器根本构建不了它们。

`--workspace` 会构建它，CI 也会。

## server

```sh
cargo build -p gproxy --release
./target/release/gproxy --version
```

```text
gproxy 4.0.0-dev
```

默认 feature 是 `channels`、`memory`、`fs` 和 `bundled-vocabulary`——一个编译进全部渠道的
单节点 SQLite 实例。逐个点名渠道可以得到一个只带用得到的上游的二进制：

```sh
cargo build -p gproxy --release --no-default-features \
  --features memory,fs,codex,claudecode,openai,custom
```

按部署需要加上 `postgres`、`mysql`、`redis` 或 `s3`。SQLite 始终编译在内，而本次构建没有的
后端会在启动时被拒绝，并指名能提供它的 feature。

## console

```sh
cd console
pnpm install --frozen-lockfile
pnpm build
```

发布构建在 `cargo build` **之前**把 `console/dist` 拷进
`crates/gproxy-host-axum/assets/web`，bundle 由 `rust-embed` 内嵌进去。

**源码检出什么都不内嵌**，这是刻意的状态：`cargo build` 产出的二进制在 console 路径上回
404，而不是一个看起来像坏掉的应用的空白页，启动日志也说了这件事。

要对着一个 Vite 构建开发，就让二进制指向一个目录：

```sh
GPROXY_CONSOLE_PATH=console/dist ./target/release/gproxy serve
```

console 所依据的 TypeScript 类型是**由 Rust 生成的**、绝不手写，从两个 crate 生成到两个
目录：

```sh
GPROXY_TS_OUT=console/src/generated/sdk cargo test -p gproxy-sdk --features ts export_types
GPROXY_TS_OUT=console/src/generated/app cargo test -p gproxy-app --features ts export_types
```

没有这个环境变量时每个测试都直接返回、什么也不写，所以 `cargo test --all-features` 保持
无副作用，而生成目录只会被有意地重写。两者去**不同**的目录，因为导出会先清空它的输出目录，
两个 crate 共用一个会把对方擦掉。

## 桌面宿主

```sh
cargo run -p gproxy-host-tauri --bin gproxy-desktop
```

一个进程、一个实例、两扇前门：承载管理面与用户面的 Tauri IPC 窗口，以及一个跑在
`127.0.0.1:7071` 上、**只提供数据面**的真正 axum 宿主，给那些会说 HTTP、不会说 IPC 的 CLI。

测试套件能在一台**没有显示服务器**的机器上驱动整套安排，因为几乎所有东西都在库里，而
二进制只负责开一个窗口。

```sh
cargo check  -p gproxy-host-tauri
cargo clippy -p gproxy-host-tauri --all-targets --all-features -- -D warnings
cargo test   -p gproxy-host-tauri
```

这个 crate 不带自动更新、不带开机自启、也不带托盘图标。这些每一个都是关于软件如何被*分发*
而不是它做什么的决定，先把它们加上意味着要为一个没有用户的应用维护一条更新通道。

## Worker

```sh
cargo install worker-build
worker-build --release -- --no-default-features --features d1,custom,codex,claudecode
```

binding、配置文档，以及那个让"逐个点名渠道"变得值得做的体积约束，见
[边缘部署（Cloudflare Workers）](/zh-cn/deployment/edge/)。

## 质量闸门

CI 跑的正是这几条：

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --workspace --target wasm32-unknown-unknown
```

最后一条不是可有可无的装饰。`gproxy-app` 和 axum router 都能为 wasm 构建，这正是 Workers
宿主能挂载同一个 router 而不是把路由表写第二遍的原因——而一个漏了 `Send` 桥接的 handler
在那个目标上就是**一个指名道姓的编译错误**。这条检查就是强制手段。

lint 报错要改代码，不是加 `#[allow]`。

在某个 crate 上干活时按 crate 跑：

```sh
cargo test   -p gproxy-channel --all-features
cargo clippy -p gproxy-channel --all-features --target wasm32-unknown-unknown --lib -- -D warnings
cargo test   -p gproxy-host-axum
cargo test   -p gproxy-store -p gproxy-seaorm
```

`gproxy-host-axum` 的每个集成测试都在一个内存实例之上构建**真正的 router**，只有上游是
脚本化的。一个直接调 handler 函数的测试会跳过正要被测的那一部分。

有两个套件必须绑定一个真实的回环端口，因为它们没法伪造：一次 WebSocket 往返，因为进程内的
service 测试装置从不产生 hyper 的升级扩展；以及一次客户端断连，因为本仓库里的每个 HTTP
客户端都会在交出 body 之前把它排空，所以那个套件直接在裸 socket 上把请求敲出去，再靠 drop
挂断。

## 本文档站

```sh
cd docs
pnpm install --frozen-lockfile
pnpm check
pnpm build
```

Astro Starlight，由 CI 部署到 Cloudflare Pages。`pnpm check` 校验本站同时承载的那份通知源。

`scripts/check-docs.sh` 是结构性检查：侧边栏 slug 对页面、中英文对等、frontmatter、
被禁止的引用，以及过长的页面。

## 这里没有什么

没有 `cargo publish`。workspace 里没有任何东西发布到 registry，因此嵌入意味着一个 git 或
路径依赖——见[嵌入核心库](/zh-cn/reference/embedding/)。公开接口尚不稳定。
