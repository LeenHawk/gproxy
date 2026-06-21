---
title: 用户与 API Key
description: 在 GPROXY v2 中管理组织、团队、用户、管理员会话、用户门户和用户 API key。
---

网关流量通过用户 API key 认证为某个 user。Console 和用户门户通过用户名/密码登录，并使用服务端 session。

v2 的身份模型是三层结构：

```text
Org
`-- Team
    `-- User
        |-- password       可选，用于 console / portal 登录
        |-- is_admin       允许访问 /admin
        `-- UserKey[]      网关流量使用的 API key
```

用户可以只有 API key、只有交互式登录密码，或者两者都有。

## 身份记录

| 记录 | 用途 |
| --- | --- |
| `orgs` | 租户边界，可以禁用。 |
| `teams` | org 内的可选分组；一个 user 可属于一个 team。 |
| `users` | 登录身份和 API key owner。 |
| `user_keys` | 网关 API key，通过 digest 加载进 control-plane snapshot。 |

热路径只索引启用的 users 和启用的 keys。Org 和 team 行也会加载，以便父级 scope 禁用时 authz 能 fail closed。

## 管理员用户

`is_admin` 控制 `/admin/*`、`/healthz`、`/version` 和 `/metrics`。管理员可以在 console 中管理 provider、route、user、rule set、settings、usage、logs 和 update。

非管理员用户使用 `/user/*` 门户查看自己的 key、limit、security、audit 和 usage。

## API Key

User key 存储内容包括：

- 用于展示/导出的加密或密封 key material；
- 用于热路径查找的 digest；
- label；
- enabled 标记。

Console 只在创建时展示明文 key。运行时请求可以按入站协议习惯传 key：

- OpenAI 风格：`Authorization: Bearer <key>`；
- Claude 风格：`x-api-key: <key>`；
- Gemini 风格：`x-goog-api-key: <key>`。

Pipeline 在 route resolution 之前认证 key，并从 `ControlPlaneSnapshot.keys_by_digest` 读取身份，避免每个请求访问持久化层。

## Session 与 Cookie

Console 登录会创建 httpOnly session cookie，session 存在 cache backend 中。本地明文 HTTP 开发需要 `GPROXY_INSECURE_COOKIES=1`。跨站部署时应配置明确的 credentialed CORS origins，不要依赖 wildcard CORS。

CSRF 默认按 same-origin 检查。Vite dev server 会代理 admin、user、health、version、metrics 路径到后端，让本地开发也能使用 same-origin cookie。

## Secret 存储

`GPROXY_MASTER_KEY` 启用 secret sealing。配置 master key 时，provider credential 和 user-key material 会以 envelope 形式存储。未配置时，v2 使用 plaintext 兼容模式并在启动/运行时警告。已有 plaintext 行仍可读取；sealed 行不能在 plaintext 模式打开。

请备份 master key。丢失后密封的 secret 无法恢复。
