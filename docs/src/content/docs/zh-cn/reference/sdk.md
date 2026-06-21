---
title: Rust SDK
description: 当前 v2 Rust library surface、feature flags，以及内部模块和发布 SDK 的边界。
---

当前 v2 tree 是一个名为 `gproxy` 的单 Rust package。它构建：

- `src/main.rs` 中的 native binary；
- `src/lib.rs` 中的 library crate，crate type 包括 `rlib` 和 `cdylib`。

不同于 v1，这个 v2 checkout 当前没有单独的已发布 crate：
`gproxy-sdk`、`gproxy-protocol`、`gproxy-channel` 或 `gproxy-engine`。除非未来增加独立 SDK package，否则应把 `gproxy` library modules 视为仓库内 integration surface。

## Library modules

`src/lib.rs` 暴露了 binary 使用的主要模块：

| Module | 职责 |
| --- | --- |
| `protocol` | provider-neutral operation taxonomy，以及 OpenAI、Claude、Gemini wire types。 |
| `transform` | 跨协议请求/响应转换和 stream adapters。 |
| `channel` | Channel trait、内置 provider adapter、auth、credential refresh、request shaping、model list helper 和 channel routing table。 |
| `pipeline` | 请求执行：auth、authz、route selection、failover、transform、上游执行、capture 和 settlement。 |
| `store` | cache 和 persistence trait/backends。 |
| `billing` and `usage` | pricing、pending quota estimate、normalized usage extraction 和 usage record。 |
| `http` | native Axum router/server 组件，以及 wasm edge request handling。 |
| `app` | bootstrap、import/export、snapshot、v1 migration、invalidation、retention 和 update status。 |
| `crypto` | 密码 hash，以及基于 `GPROXY_MASTER_KEY` 的 secret seal/open。 |
| `admin` and `api` | 跨 target admin guard 和 API helper。 |
| `selfupdate` | native-only 自更新实现。 |

这些模块对贡献者和 embedding 实验有用，但当前不应被视为稳定 semver SDK contract。

## Feature flags

package 级 feature flags 以 backend 为主：

| Feature | 用途 |
| --- | --- |
| `default` | native 默认：memory cache、db 和 file persistence、wreq upstream client、本地计数、v1 migration。 |
| `full` | native 便利 feature，启用所有 native backend。 |
| `cache-memory` | 进程内 cache backend。 |
| `cache-redis` | Redis cache backend，用于多实例 cache/invalidation。 |
| `persist-file` | 本地 JSON-file persistence backend。 |
| `persist-db` | SeaORM database persistence backend。 |
| `migrate-v1` | legacy v1 SQLite migration reader 和 serve-path auto-migration hook。 |
| `upstream-wreq` | native HTTP upstream client。 |
| `count-local` | 通过 tokenizer 依赖支持 native 本地 token counting。 |
| `cache-libsql`, `cache-upstash`, `persist-libsql`, `upstream-fetch` | wasm/edge backend gates。 |
| `edge` | wasm edge backend set 的 umbrella feature。 |

## Embedding 边界

binary 很薄：解析 CLI/env 配置，构建 persistence/cache/client/channel registry/state，按需运行 import/export、migration/update 子命令，然后启动 HTTP router。

如果直接 embed library，需要自己组装同样的对象：

1. 构建 `RuntimeConfig`。
2. 打开 `PersistenceBackend`。
3. 构建 `SecretCipher`。
4. 构建 `CacheBackend`。
5. 构建 `ChannelRegistry`。
6. 构建 `AppState` 和 control-plane snapshot。
7. 调用 HTTP router 或更底层的 pipeline 函数。

当前还没有封装成小型 public builder API。

## Protocol 与 operation taxonomy

v2 稳定的概念中心是 operation taxonomy：

- `Operation`：`list_models`、`get_model`、`count_tokens`、
  `generate_content`、`stream_generate_content`、`create_image`、
  `edit_image`、`create_embedding`、`compact_content` 和
  `create_conversation`；
- `OperationGroup`：models、count tokens、generate content、images、
  embeddings、compact 和 conversation；
- `OperationKind`：provider family（`open_ai`、`claude`、`gemini`）或
  content-generation wire kind（`open_ai_responses`、
  `open_ai_chat_completions`、`claude_messages`、`gemini_generate_content`）。

routing rules、transforms、endpoint synthesis 和 settlement 都围绕这个 taxonomy 构建。

## 当前建议

生产使用建议运行 `gproxy` binary 或 edge bundle。library surface 更适合开发、测试、本仓库内定制部署，或可以跟随内部 API 变化的实验。
