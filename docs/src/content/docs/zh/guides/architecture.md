---
title: 架构
description: 项目结构、启动流程、请求生命周期与凭据健康机制。
---

## 项目结构

| 路径 | Crate | 职责 |
|---|---|---|
| `apps/gproxy` | `gproxy` | 二进制入口。Axum 服务器，通过 `rust-embed` 嵌入控制台，CLI 参数解析，后台任务编排。 |
| `sdk/gproxy-protocol` | `gproxy-protocol` | OpenAI、Claude、Gemini 请求/响应类型定义。六种协议之间的跨协议转换逻辑。 |
| `sdk/gproxy-provider` | `gproxy-provider` | `Channel` trait 及全部 14 种内置 Channel 实现。Dispatch 表、凭据重试循环、计费/定价表、健康追踪、OAuth 流程。 |
| `sdk/gproxy-routing` | `gproxy-routing` | 从 HTTP 路径到 `(OperationFamily, ProtocolKind)` 的路由分类。模型别名解析、权限检查、速率限制规则、模型名提取与规范化。 |
| `sdk/gproxy-sdk` | `gproxy-sdk` | 顶层 SDK crate，重导出 `gproxy-protocol`、`gproxy-provider` 和 `gproxy-routing`。 |
| `crates/gproxy-core` | `gproxy-core` | 领域服务：Config（配置）、Identity（用户/Key 管理）、Policy（权限策略）、Quota（Token 预算）、Routing（提供商解析）、File（文件 API 代理）。 |
| `crates/gproxy-server` | `gproxy-server` | `AppState` 构建、全局配置、中间件栈、会话管理、价格等级定义。 |
| `crates/gproxy-api` | `gproxy-api` | Admin、User、Provider 路由的 HTTP 处理器。引导逻辑（从 TOML 种子、生成默认值、从数据库重载）。登录和认证端点。 |
| `crates/gproxy-storage` | `gproxy-storage` | SeaORM 实体定义、仓储实现、Schema 同步、异步写入 sink（`StorageWriteEvent` channel，非阻塞持久化）。 |
| `frontend/console` | `@gproxy/console` | React 管理控制台。Vite + Tailwind。构建产物在编译时嵌入二进制文件。 |

## 启动流程

运行 `./gproxy` 时，按以下顺序执行：

1. **解析 CLI 参数。** Host、Port、DSN、管理员凭据、配置文件路径、代理、Spoof 模拟。所有参数均支持环境变量覆盖。

2. **解析 DSN。** 如果未指定 `--dsn`，默认使用 `sqlite://./data/gproxy.db?mode=rwc`。如果数据库中已有全局设置且指向不同的 DSN，GPROXY 会重新连接到该数据库。

3. **连接数据库 + 同步 Schema。** 通过 SeaORM 建立数据库连接。运行 Schema 同步（创建表、为迁移添加新列）。无需单独的迁移 CLI，完全自动化。

4. **决策：重载还是种子。** 如果数据库已有全局设置，GPROXY 从数据库加载所有状态（提供商、用户、Key、模型）。如果数据库为空，则查找 `gproxy.toml`：
   - **TOML 存在：** 解析并将提供商/凭据/用户/模型写入数据库。
   - **无 TOML：** 创建最小默认值（空提供商列表）。

5. **协调引导管理员。** 确保 `id=0` 的用户（管理员）存在。如果通过 CLI 参数提供了管理员凭据则应用它们。如果没有且为首次运行，则生成随机密码和 API key 并输出到标准输出。

6. **启动后台任务：**
   - **用量 sink** -- 从 mpsc channel 读取，批量将用量记录写入数据库。
   - **配额协调器** -- 定期根据累计用量同步配额余额。
   - **速率限制 GC** -- 清理过期的速率限制窗口。
   - **健康广播器** -- 订阅凭据健康变更，通过 WebSocket 推送更新到已连接的控制台客户端。

7. **绑定 Axum 服务器。** 合并 API 路由（Admin/User/Provider 路由）和控制台路由（嵌入式 SPA）。在 `host:port` 上开始监听，支持 SIGTERM/Ctrl+C 优雅关闭。

## 请求生命周期

发往 GPROXY 的请求经过以下路径：

1. **认证。** 从 `Authorization: Bearer ...` 或 `x-api-key` 头中提取 API key。查找身份信息（用户 + Key）。检查 Key 是否启用、用户是否活跃、是否已超出配额。

2. **路由分类。** 将 URL 路径解析为 `(OperationFamily, ProtocolKind)`。路径分为：
   - **无作用域：** `/v1/chat/completions` -- GPROXY 从请求体中的模型名称解析提供商。
   - **指定提供商：** `/my-provider/v1/chat/completions` -- 第一个路径段直接指定提供商。

3. **Dispatch 表查找。** 根据解析出的提供商 Channel，在 dispatch 表中查找 `(operation, protocol)` 对。返回 `Passthrough`、`TransformTo { destination }`、`Local` 或 `Unsupported` 之一。

4. **协议转换（请求侧）。** 如果 dispatch 结果为 `TransformTo`，将请求体从源协议转换为目标协议。例如，OpenAI Chat Completions 请求体转换为 Claude Messages 请求体。

5. **后缀处理。** 去除模型后缀（如 `-thinking`、`-fast`、`-1m`）并应用其效果（启用扩展思考、设置速度提示、调整上下文窗口参数）。后缀同时从协议级别分组和 Channel 级别分组中解析。

6. **`finalize_request`。** Channel 特有的请求体规范化，对路由和 Cache Affinity 逻辑可见。在协议转换之后、凭据选择之前执行。

7. **凭据选择。** 从提供商的凭据池中选取一个健康的凭据。两种选择模式：
   - **Round-robin**（默认）-- 在健康凭据间轮转。
   - **Cache Affinity** -- 对请求内容做哈希，将相似 prompt 固定到同一凭据，最大化上游 prompt 缓存命中率。
   此步骤会检查速率限制窗口。如果某个凭据的速率限制已耗尽，跳到下一个。

8. **`prepare_request`。** 构建实际 HTTP 请求：设置 URL，注入认证头，应用 Channel 特有的传输封装（API key 头、OAuth bearer token、请求 ID）。

9. **HTTP 发送。** 通过相应的 HTTP 客户端发送请求。需要浏览器模拟 TLS 的 Channel（基于 Cookie 的认证）使用 Spoof 客户端。

10. **`classify_response`。** 检查 HTTP 状态码、头部和响应体，判断结果：
    - **Success** -- 继续处理。
    - **RateLimit** -- 标记凭据冷却，尝试下一个凭据。
    - **AuthDead** -- 尝试刷新凭据（OAuth token 轮换）。如果刷新成功，重试一次。如果仍然失败，标记凭据不可用，尝试下一个。
    - **TransientError** -- 标记凭据短暂冷却，尝试下一个。
    - **PermanentError** -- 向客户端返回错误。

11. **重试/故障转移。** 遇到可重试错误时，回到第 7 步使用下一个凭据。对于无 `retry-after` 的 429 响应，每个凭据的重试次数上限可配置（默认 3 次）。

12. **`normalize_response`。** 协议转换前 Channel 特有的响应体修正。例如，DeepSeek 将 `insufficient_system_resource` finish reason 映射为 `length`，Vertex Channel 会展开信封包装。

13. **协议转换（响应侧）。** 如果原始 dispatch 结果为 `TransformTo`，将响应体从上游协议转换回客户端期望的协议。

14. **用量记录。** 从响应中提取 token 用量。将用量记录推送到异步写入 sink（非阻塞）。用量 sink worker 批量写入数据库。

## 凭据健康

提供商凭据池中的每个凭据都有健康状态：

- **healthy** -- 可用于选择。无活跃冷却。
- **cooldown** -- 暂时不可用。全局冷却或按模型冷却正在生效。选择时跳过该凭据，但会自动恢复。
- **unavailable (dead)** -- 上游返回 401/403 且凭据刷新失败。完全排除出选择范围，直到手动重新启用或刷新成功。

### 冷却行为

当 429 响应**带有** `retry-after` 头时，冷却时长精确匹配服务端返回的值。

当 429 响应**不带** `retry-after` 时，冷却采用有上限的指数退避：1s、2s、4s、8s……最大 60s。下次成功后退避计数器归零。

冷却可以是全局的（所有模型）或按模型的，取决于失败请求的目标模型。如果请求指定了模型，只有该模型在该凭据上进入冷却——其他模型仍然可用。这就是「部分」状态：凭据对大多数模型健康，但对特定模型暂时冷却中。
