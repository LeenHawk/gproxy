---
title: 发行版构建
description: 如何生成 GPROXY 的生产二进制 —— 包括内嵌控制台。
---

生产构建 = 一次 Cargo release 构建 + 一次前端构建。两个步骤都是幂等的，
可以放进 CI。

## 1. 构建内嵌控制台 (若有变更)

如果你改过 `frontend/console/` 下的任何内容，先构建控制台，再构建二进制：

```bash
cd frontend/console
pnpm install
pnpm build
cd ../..
```

`pnpm build` 会把产物写到 server crate 通过 `include_dir!` 嵌入的目录 ——
无需单独部署静态文件。

如果没改前端可以跳过这一步 —— 上一次构建的产物会被本次 Cargo 构建自动拾取。

## 2. 构建 Rust 二进制

```bash
cargo build -p gproxy --release
```

产物位于 `target/release/gproxy`。它静态链接 Rust 标准库，但默认依赖系统的
OpenSSL / TLS 栈 (除非你启用 `rustls` 相关 feature)。

## 3. strip 并打包 (可选)

若要减小分发体积：

```bash
strip target/release/gproxy
```

之后直接分发该 strip 过的二进制即可 —— 除 `libc` 和 TLS 栈外无运行时依赖。

## 4. 首次启动

首次启动时 GPROXY 会：

- 若 `GPROXY_DATA_DIR` 不存在则创建。
- 若未设置 `GPROXY_DSN`，在数据目录下生成 SQLite 文件。
- 若数据库为空，导入种子 TOML (`GPROXY_CONFIG`)。
- 必要时从 `GPROXY_ADMIN_*` bootstrap 管理员，自动生成的凭证会**打印一次**。
- 启动 HTTP 服务与 worker set。

具体首次启动的细节见 [快速开始](/zh-cn/getting-started/quick-start/)。

## CI 建议

- 缓存 `~/.cargo` 与 `target/` 目录 —— 工作空间依赖很多，冷构建最耗时的就是重新下载。
- 同样缓存 pnpm store 下的 `frontend/console/node_modules`。
- 在 PR 上跑 `cargo test -p gproxy` 和 `pnpm test` (如已配置)；`release` 构建留给 tag 触发的流水线。
- 打 tag 时可从 `RELEASE_NOTE.md` 生成发布说明，并把 strip 过的二进制作为构建产物附加上去。
