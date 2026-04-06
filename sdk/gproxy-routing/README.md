# gproxy-routing

`gproxy-routing` 是一个不绑定具体 Web 框架的路由纯逻辑库。它关注的是“如何从路径、Header、Body 和模型标识中提取路由决策”，而不是 HTTP server 本身。

## 设计目标

| 设计点 | 说明 |
| --- | --- |
| 无框架依赖 | `Cargo.toml` 只依赖 `gproxy-protocol`、`http`、`serde`、`serde_json`、`regex`、`thiserror`、`tracing`，没有 `axum`、`warp`、`actix-web` 之类框架依赖。 |
| 输入形态稳定 | 公开函数主要接受 `&str`、`&[u8]`、`http::Method`、`http::HeaderMap` 等通用类型。 |
| 适合复用 | 同一套 helper 可用于网关、测试、代理层、中间件或 CLI 工具。 |
| 易于测试 | 逻辑主要是字符串、JSON、Header 和枚举匹配，基本不依赖运行时上下文。 |

## 关键公开类型

| 类型 | 位置 | 用途 |
| --- | --- | --- |
| `Classification` | `src/classify.rs` | 路由分类结果，包含 `operation`、`protocol` 和 `is_stream`。 |
| `RoutingError` | `src/error.rs` | 路由 helper 统一错误类型。 |
| `ModelAliasTarget` | `src/model_alias.rs` | 模型别名解析后的目标 provider 与模型。 |
| `PermissionEntry` | `src/permission.rs` | 模型权限项。 |
| `FilePermissionEntry` | `src/permission.rs` | 文件 API 的 provider 级权限项。 |
| `RateLimitRule` | `src/rate_limit.rs` | 模型级限流规则。 |
| `OperationFamily` | `gproxy-protocol::kinds` | 路由层复用的操作族枚举。 |
| `ProtocolKind` | `gproxy-protocol::kinds` | 路由层复用的协议枚举。 |

## 公开函数列表

| 函数 | 签名 | 用途 |
| --- | --- | --- |
| `classify_route` | `fn classify_route(method: &Method, path: &str, headers: &HeaderMap, body: Option<&[u8]>) -> Result<Classification, RoutingError>` | 仅依赖 HTTP 元数据和可选 body，对请求进行协议与操作分类。 |
| `normalize_path` | `fn normalize_path(path: &str) -> String` | 去掉版本前缀、重复斜杠和尾随斜杠，统一路径。 |
| `extract_model_from_uri_path` | `fn extract_model_from_uri_path(path: &str) -> Option<String>` | 从 URI 路径中提取模型名。 |
| `extract_model` | `fn extract_model(uri_path: &str, body: Option<&[u8]>, operation: OperationFamily, protocol: ProtocolKind) -> Option<String>` | 按操作和协议决定从 URI 还是 JSON body 提取模型。 |
| `pattern_matches` | `fn pattern_matches(pattern: &str, model: &str) -> bool` | 以 `*`、前缀或后缀规则匹配模型字符串。 |
| `check_model_permission` | `fn check_model_permission(permissions: &[PermissionEntry], provider_id: Option<i64>, model: &str) -> bool` | 判断权限列表是否允许访问某 provider/model。 |
| `split_provider_prefixed_model` | `fn split_provider_prefixed_model(value: &str) -> Option<(bool, &str, &str)>` | 把 `models/provider/model` 或 `provider/model` 拆成 `(has_models_prefix, provider, model)`。 |
| `add_provider_prefix` | `fn add_provider_prefix(value: &str, provider: &str) -> String` | 为模型字符串补上 provider 前缀。 |
| `strip_provider_from_body` | `fn strip_provider_from_body(operation: OperationFamily, protocol: ProtocolKind, body: &[u8]) -> Option<(String, Vec<u8>)>` | 从 JSON body 的模型字段中剥离 provider 前缀，并返回 provider 名与新 body。 |
| `strip_provider_from_uri_path` | `fn strip_provider_from_uri_path(path: &str) -> Option<(String, String)>` | 从 URI 路径里的模型标识中剥离 provider 前缀。 |
| `find_matching_rule` | `fn find_matching_rule<'a>(rules: &'a [RateLimitRule], model: &str) -> Option<&'a RateLimitRule>` | 按“精确匹配 > 最长前缀 > `*`”查找最合适的限流规则。 |
| `sanitize_headers` | `fn sanitize_headers(headers: &mut HeaderMap)` | 删除敏感或浏览器上下文 Header，再向上游转发。 |
| `sanitize_query_params` | `fn sanitize_query_params(path_and_query: &str) -> String` | 从 `path?query` 中移除认证类查询参数。 |

## 模块分工

| 模块 | 公开项 | 说明 |
| --- | --- | --- |
| `classify` | `Classification`, `classify_route`, `normalize_path`, `extract_model_from_uri_path` | 路径与协议分类。 |
| `model_extraction` | `extract_model` | 模型提取。 |
| `permission` | `PermissionEntry`, `FilePermissionEntry`, `pattern_matches`, `check_model_permission` | 权限匹配。 |
| `provider_prefix` | `split_provider_prefixed_model`, `add_provider_prefix`, `strip_provider_from_body`, `strip_provider_from_uri_path` | provider 前缀处理。 |
| `rate_limit` | `RateLimitRule`, `find_matching_rule` | 限流规则匹配。 |
| `sanitize` | `sanitize_headers`, `sanitize_query_params` | Header / Query 清洗。 |
| `error` | `RoutingError` | 错误定义。 |
| `model_alias` | `ModelAliasTarget` | 模型别名目标类型。 |
