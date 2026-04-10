---
title: 认证与授权
description: 登录流程、session token、API key 提取、管理员与用户权限边界。
---

## 鉴权模型概览

GPROXY 有两种凭证类型和两个角色层级：

- **Session token** -- 用于控制台/管理操作。通过 `/login` 获取。
- **API key** -- 用于代理流量。由管理员或用户自助 API 按用户管理。

角色层级：

- **Admin** -- `is_admin = true` 的用户。可访问 `/admin/*` 路由。
- **User** -- 任意用户。可使用 session token 访问 `/user/*` 路由，使用 API key 调用 provider 代理路由。

## 登录

```
POST /login
Content-Type: application/json

{
  "username": "alice",
  "password": "secret"
}
```

响应：

```json
{
  "user_id": 1,
  "session_token": "sess-abc123...",
  "is_admin": false,
  "expires_in_secs": 86400
}
```

### Session token

- 以 `sess-` 为前缀。
- 24 小时有效期。
- 仅存于内存——不持久化到数据库。服务重启后所有 session 失效。
- 用于 `/admin/*` 和 `/user/*` 管理路由。

Session token 与 API key 刻意分离，确保泄露的推理 key 无法用于管理账户（创建 key、查看用量等）。

## API key 提取

对于 provider 代理路由，GPROXY 按以下顺序从请求中提取 API key：

1. `Authorization: Bearer <key>` 请求头
2. `x-api-key: <key>` 请求头
3. `x-goog-api-key: <key>` 请求头
4. `?key=<key>` 查询参数

取第一个非空值。如果全部为空，请求被 401 拒绝。

## 路由授权

### `/admin/*` 路由

需要以下之一：

- 属于管理员用户的 session token（`sess-*`）。
- 属于管理员用户的 API key。

非管理员 token 或 key 返回 403 Forbidden。

### `/user/*` 路由

需要 session token（`sess-*`）。API key 不被接受。

这是有意设置的安全边界：泄露的推理 API key 无法在 `/user/*` 自助路由上列举其他 key、创建新 key 或查询用量。

### Provider 代理路由

需要有效且启用的 API key。Session token 不被接受。Key 标识用户身份，用于权限检查、速率限制和配额控制。

## Admin 与 User 权限边界

| 能力 | Admin（session 或 admin key） | User（session） | User（API key） |
|------|-------------------------------|-----------------|-----------------|
| 全局设置 | 读/写 | -- | -- |
| Provider 管理 | 创建/更新/删除 | -- | -- |
| Credential 管理 | 创建/更新/删除 | -- | -- |
| 用户管理 | 创建/更新/删除所有用户 | -- | -- |
| 自有 API key 管理 | -- | 创建/删除自有 key | -- |
| 自有用量查询 | -- | 查看自有数据 | -- |
| Provider 代理调用 | -- | -- | 可用 |
| 模型列表/详情 | -- | -- | 可用 |
| 用量追踪 | 查看所有用户 | 查看自有数据 | 按请求记录 |
| 配置导入导出 | 可用 | -- | -- |
| 系统更新 | 可用 | -- | -- |

### 建议

- Admin key 仅用于配置和运维，不用于推理流量。
- 为每个团队、服务或环境发放独立的 user API key，便于审计。
- 在全局设置中启用日志标志（`enable_upstream_log`、`enable_downstream_log`）以留存审计记录。
- 生产环境中保持 `enable_upstream_log_body` 和 `enable_downstream_log_body` 关闭，除非正在调试——它们会记录完整的请求/响应体。
