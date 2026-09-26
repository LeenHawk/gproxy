---
title: 安装
description: 从源码构建 GPROXY v4 的二进制、运行它、找到它的数据，并在 server、桌面宿主与 Workers 宿主之间做选择。
---

v4 **没有发布流水线**：没有安装包、没有便携包、没有发布的容器镜像、也没有签名产物。
拿到二进制只有一条受支持的路径，就是自己构建一个。

:::note[下载页去哪了]
v3 为四个平台发布过安装包和一份签名的更新清单。这些都没有被移植，本站也不再描述它们。
描述过它们的三个页面——下载、代码签名和容器部署——被删除，而不是围绕不存在的机制改写。
:::

## 前置条件

| 工具 | 用于 |
| --- | --- |
| stable Rust 工具链（edition 2024） | 每个 crate |
| `wasm32-unknown-unknown` | 只有 Workers 宿主需要 |
| `webkit2gtk-4.1`、`gtk+-3.0`、`libsoup-3.0`（Linux） | 只有桌面宿主需要 |
| Node.js LTS 与 pnpm | 只有 console 包需要 |

本仓库最近一次在这里构建时使用的版本：

```text
rustc 1.98.0 (88d9e12ae 2026-08-18)
cargo 1.98.0 (797e8a9bc 2026-08-05)
```

## 构建 server

```sh
git clone https://github.com/LeenHawk/gproxy
cd gproxy
cargo build -p gproxy --release
```

二进制在 `target/release/gproxy`。

```text
$ ./target/release/gproxy --version
gproxy 4.0.0-dev
```

默认 feature 集是一个单节点 SQLite 实例，带本仓库实现的全部渠道：`channels`、`memory`、
`fs`、`bundled-vocabulary`。

| Feature | 默认 | 增加什么 |
| --- | --- | --- |
| `channels` | ✓ | 全部 25 个渠道。逐个点名（`codex`、`kiro`、`openai`…）可以只带用得到的上游 |
| `memory` | ✓ | 进程内 cache |
| `fs` | ✓ | 本地文件存储 |
| `bundled-vocabulary` | ✓ | 一份兜底的 tokenizer 词表 |
| `postgres` | | PostgreSQL 驱动 |
| `mysql` | | MySQL 驱动 |
| `redis` | | 多实例部署所需的共享 cache |
| `s3` | | S3 兼容的文件存储 |

SQLite 始终编译在内。本次构建没有的后端会在**启动时**被拒绝，并指名能提供它的 feature，
而不是等到第一个请求。

## 运行

```sh
./target/release/gproxy serve --data-dir ./data --port 8787
```

首次启动会建库、建表，创建一个管理员、签发一把网关 API key，并把两者**只打印一次**到
标准输出：

```text
GPROXY first-run administrator (shown once)
  user:     admin
  password: p0Wsgf70xcFViWtn1UhZQ3msZSYHx2ZC
  api key:  sk-5hKlHHF0mtyw4pQewEj6pD2WZPkgH2vN-5lufKRtTdQ
Save these before closing this terminal; they are not stored in a form
this instance can show you again.
```

请保存下来。密码经 argon2 哈希、key 只存 SHA-256 摘要，实例再也拿不出来了。

日志走标准**错误**，这正是让上面那段和 `gproxy export --out -` 保持干净的原因：

```text
WARN gproxy::serve: upstream credential secrets are stored UNENCRYPTED: no
     master key is configured. Set GPROXY_MASTER_KEY to 32 bytes as 64 hex
     characters or base64 …
INFO gproxy::serve: no console bundle is compiled into this binary and no
     directory was named, so /console answers 404. …
INFO gproxy::bootstrap: created the first administrator user="admin"
INFO gproxy::serve: gproxy is listening address=127.0.0.1:8787 revision=2 console=false
```

这两行都是真的，也都是有意为之。往下看。

### 在第一把凭证之前设好主密钥

没有主密钥时，上游凭证密钥以**明文**存放。这是一种受支持的部署形态——二进制在启动时会
大声说出来——但不该是不小心保持的那一种。

```sh
GPROXY_MASTER_KEY="$(openssl rand -hex 32)" \
  ./target/release/gproxy serve --data-dir ./data
```

32 字节，写成 64 个十六进制字符或 base64。在已经有凭证*之后*才设置它属于轮换而不是改配置，
见[配置](/zh-cn/reference/configuration/#主密钥轮换)。

### 源码检出没有 console

`/console` 由编译进二进制的一份 bundle 提供，而**源码检出什么都没内嵌**。这是刻意的状态：
`cargo build` 产出的二进制在 console 路径上回 404，而不是一个看起来像坏掉的应用的空白页，
启动日志也说了这件事。

要有 console，就构建它并让二进制指过去：

```sh
cd console && pnpm install && pnpm build
GPROXY_CONSOLE_PATH=console/dist ./target/release/gproxy serve
```

发布构建的做法则是在 `cargo build` 之前把 `console/dist` 拷进
`crates/gproxy-host-axum/assets/web`，bundle 就被内嵌进去。

## 数据在哪

一切相对路径都相对 `--data-dir`（`GPROXY_DATA_DIR`，默认 `data`）解析：

| 路径 | 是什么 |
| --- | --- |
| `<data-dir>/gproxy.db` | SQLite 实例 |
| `<data-dir>/` 加 `--file-storage-dir` | 发布的 body 与下载的词表 |

用 `postgres` 或 `mysql` 时这个目录仍然存在，用于文件存储。

`migrate` 创建或增量同步表结构后退出：

```text
$ ./target/release/gproxy migrate --data-dir ./data
INFO gproxy::instance: schema is up to date warnings=0
```

`serve` 也会做这件事。单独的命令是给那些把迁移当作独立一步、**由单一写入者**在任何实例
启动前执行的部署用的——多实例共用一个数据库正需要这样。

## 桌面宿主

`gproxy-desktop` 是同一个实例之上的 Tauri 窗口，v4 新增。它不在 workspace 的默认成员里，
因为它会拉进 webkit、gtk 和大约一百八十个与引擎无关的 crate。

```sh
cargo run -p gproxy-host-tauri --bin gproxy-desktop
```

一个实例，两扇前门：

- **窗口**，经 Tauri IPC，承载管理面与用户面。这里刻意没有认证：消息之所以到达，只因为
  本进程自己的 webview 发出了它，通道本身就是证明。
- **`127.0.0.1:8787`**，一个真正的 axum 宿主，**只提供数据面**，给只会说 HTTP、不会说 IPC
  的 Claude Code 与 Codex CLI。它**照常要网关 key**——回环 socket 不是信任边界——并且
  `/admin/api` 与 `/portal/api` 在那里返回 404，这样网关 key 就不会同时是一把管理员钥匙。

默认端口为 8787，与 server 一致。同机同时运行多个实例时，可在 `gproxy.toml` 中修改 `port`。

主密钥在首次运行时生成，存进平台钥匙串（Linux 的 Secret Service、macOS 的 Keychain、
Windows 的凭据管理器）。它**不会**退化成文件——一把躺在它所保护的数据库旁边的钥匙，
不是加密，只是通往同一份明文的更长的路。没有钥匙串时，实例的行为与没有
`GPROXY_MASTER_KEY` 的 server 完全一致，并且会这么说。

## 一个 Cloudflare Worker

第三个宿主把同一个 router 编译到 wasm。见
[边缘部署（Cloudflare Workers）](/zh-cn/deployment/edge/)。

## 下一步

- [快速开始](/zh-cn/getting-started/quick-start/)——一个 Provider、一把凭证、一条路由、一个请求。
- [配置](/zh-cn/reference/configuration/)——每个 flag 与每个 `GPROXY_*` 变量。
- [从源码构建](/zh-cn/deployment/release-build/)——质量闸门与其余构建目标。
