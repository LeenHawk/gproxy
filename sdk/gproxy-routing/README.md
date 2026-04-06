# gproxy-routing / gproxy-routing

[中文](#中文) | [English](#english)

---

## 中文

`gproxy-routing` 是一个不绑定具体 Web 框架的路由纯逻辑库。它关注的是“如何从路径、Header、Body 和模型标识中提取路由决策”，而不是 HTTP server 本身。

### 设计目标

下表是中英共享表格，概括这个 crate 的设计取向。

| 设计点 / Design Goal | 说明 / Description |
| --- | --- |
| 无框架依赖 / Framework agnostic | `Cargo.toml` 只依赖 `gproxy-protocol`、`http`、`serde`、`serde_json`、`regex`、`thiserror`、`tracing`，没有 `axum`、`warp`、`actix-web` 之类框架依赖。 / `Cargo.toml` depends only on `gproxy-protocol`, `http`, `serde`, `serde_json`, `regex`, `thiserror`, and `tracing`, without framework dependencies such as `axum`, `warp`, or `actix-web`. |
| 输入形态稳定 / Stable input surface | 公开函数主要接受 `&str`、`&[u8]`、`http::Method`、`http::HeaderMap` 等通用类型。 / Public functions mainly accept generic types such as `&str`, `&[u8]`, `http::Method`, and `http::HeaderMap`. |
| 适合复用 / Reusable | 同一套 helper 可用于网关、测试、代理层、中间件或 CLI 工具。 / The same helper set can be reused in gateways, tests, proxy layers, middleware, or CLI tools. |
| 易于测试 / Easy to test | 逻辑主要是字符串、JSON、Header 和枚举匹配，基本不依赖运行时上下文。 / The logic is mostly string, JSON, header, and enum matching, with minimal runtime coupling. |

### 关键公开类型

下表列出路由层的关键公开类型，供中英文两部分共用。

| 类型 / Type | 位置 / Location | 用途 / Purpose |
| --- | --- | --- |
| `Classification` | `src/classify.rs` | 路由分类结果，包含 `operation`、`protocol` 和 `is_stream`。 / Route classification result containing `operation`, `protocol`, and `is_stream`. |
| `RoutingError` | `src/error.rs` | 路由 helper 统一错误类型。 / Unified error type for routing helpers. |
| `ModelAliasTarget` | `src/model_alias.rs` | 模型别名解析后的目标 provider 与模型。 / Target provider and model after resolving a model alias. |
| `PermissionEntry` | `src/permission.rs` | 模型权限项。 / Model permission entry. |
| `FilePermissionEntry` | `src/permission.rs` | 文件 API 的 provider 级权限项。 / Provider-level permission entry for file APIs. |
| `RateLimitRule` | `src/rate_limit.rs` | 模型级限流规则。 / Model-level rate-limit rule. |
| `OperationFamily` | `gproxy-protocol::kinds` | 路由层复用的操作族枚举。 / Operation-family enum reused by the routing layer. |
| `ProtocolKind` | `gproxy-protocol::kinds` | 路由层复用的协议枚举。 / Protocol enum reused by the routing layer. |

### 公开函数列表

下表汇总主要公开函数、签名和用途。

| 函数 / Function | 签名 / Signature | 用途 / Purpose |
| --- | --- | --- |
| `classify_route` | `fn classify_route(method: &Method, path: &str, headers: &HeaderMap, body: Option<&[u8]>) -> Result<Classification, RoutingError>` | 仅依赖 HTTP 元数据和可选 body，对请求进行协议与操作分类。 / Classifies protocol and operation using only HTTP metadata and an optional body. |
| `normalize_path` | `fn normalize_path(path: &str) -> String` | 去掉版本前缀、重复斜杠和尾随斜杠，统一路径。 / Normalizes a path by removing version prefixes, duplicate slashes, and trailing slashes. |
| `extract_model_from_uri_path` | `fn extract_model_from_uri_path(path: &str) -> Option<String>` | 从 URI 路径中提取模型名。 / Extracts the model name from the URI path. |
| `extract_model` | `fn extract_model(uri_path: &str, body: Option<&[u8]>, operation: OperationFamily, protocol: ProtocolKind) -> Option<String>` | 按操作和协议决定从 URI 还是 JSON body 提取模型。 / Chooses whether to extract the model from the URI or JSON body based on operation and protocol. |
| `pattern_matches` | `fn pattern_matches(pattern: &str, model: &str) -> bool` | 以 `*`、前缀或后缀规则匹配模型字符串。 / Matches model strings using `*`, prefix, or suffix rules. |
| `check_model_permission` | `fn check_model_permission(permissions: &[PermissionEntry], provider_id: Option<i64>, model: &str) -> bool` | 判断权限列表是否允许访问某 provider/model。 / Checks whether the permission list allows access to a given provider/model pair. |
| `split_provider_prefixed_model` | `fn split_provider_prefixed_model(value: &str) -> Option<(bool, &str, &str)>` | 把 `models/provider/model` 或 `provider/model` 拆成 `(has_models_prefix, provider, model)`。 / Splits `models/provider/model` or `provider/model` into `(has_models_prefix, provider, model)`. |
| `add_provider_prefix` | `fn add_provider_prefix(value: &str, provider: &str) -> String` | 为模型字符串补上 provider 前缀。 / Adds a provider prefix to a model string. |
| `strip_provider_from_body` | `fn strip_provider_from_body(operation: OperationFamily, protocol: ProtocolKind, body: &[u8]) -> Option<(String, Vec<u8>)>` | 从 JSON body 的模型字段中剥离 provider 前缀，并返回 provider 名与新 body。 / Removes the provider prefix from the model field in a JSON body and returns the provider name with the rewritten body. |
| `strip_provider_from_uri_path` | `fn strip_provider_from_uri_path(path: &str) -> Option<(String, String)>` | 从 URI 路径里的模型标识中剥离 provider 前缀。 / Removes the provider prefix from the model identifier in a URI path. |
| `find_matching_rule` | `fn find_matching_rule<'a>(rules: &'a [RateLimitRule], model: &str) -> Option<&'a RateLimitRule>` | 按“精确匹配 > 最长前缀 > `*`”查找最合适的限流规则。 / Finds the best matching rate-limit rule using “exact match > longest prefix > `*`”. |
| `sanitize_headers` | `fn sanitize_headers(headers: &mut HeaderMap)` | 删除敏感或浏览器上下文 Header，再向上游转发。 / Removes sensitive or browser-context headers before forwarding upstream. |
| `sanitize_query_params` | `fn sanitize_query_params(path_and_query: &str) -> String` | 从 `path?query` 中移除认证类查询参数。 / Removes authentication-related query parameters from `path?query`. |

### 模块分工

下表说明各模块的公开项与职责。

| 模块 / Module | 公开项 / Public Items | 说明 / Description |
| --- | --- | --- |
| `classify` | `Classification`, `classify_route`, `normalize_path`, `extract_model_from_uri_path` | 路径与协议分类。 / Path and protocol classification. |
| `model_extraction` | `extract_model` | 模型提取。 / Model extraction. |
| `permission` | `PermissionEntry`, `FilePermissionEntry`, `pattern_matches`, `check_model_permission` | 权限匹配。 / Permission matching. |
| `provider_prefix` | `split_provider_prefixed_model`, `add_provider_prefix`, `strip_provider_from_body`, `strip_provider_from_uri_path` | provider 前缀处理。 / Provider-prefix handling. |
| `rate_limit` | `RateLimitRule`, `find_matching_rule` | 限流规则匹配。 / Rate-limit rule matching. |
| `sanitize` | `sanitize_headers`, `sanitize_query_params` | Header / Query 清洗。 / Header and query sanitization. |
| `error` | `RoutingError` | 错误定义。 / Error definitions. |
| `model_alias` | `ModelAliasTarget` | 模型别名目标类型。 / Model alias target type. |

---

## English

`gproxy-routing` is a pure routing-logic crate that is not tied to any specific web framework. Its focus is how to derive routing decisions from paths, headers, bodies, and model identifiers rather than on the HTTP server itself.

### Design Goals

See the shared bilingual table above for the crate's design goals.

### Key Public Types

See the shared bilingual table above for the key public types exposed by the routing layer.

### Public Function List

See the shared bilingual table above for the main public functions, their signatures, and their responsibilities.

### Module Responsibilities

See the shared bilingual table above for how the modules are split and which public items each module owns.
