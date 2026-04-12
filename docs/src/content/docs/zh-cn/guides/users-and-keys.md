---
title: 用户与 API 密钥
description: GPROXY 中用户、API 密钥和管理员账号的组织方式。
---

GPROXY 原生就是多租户的。每个请求都必须以**用户**身份鉴权，每个用户可以持有
多个 **API 密钥**。

## 数据模型

```text
User
├── name            (唯一)
├── password        (Argon2 PHC；可选；用于控制台登录)
├── is_admin        (bool)
├── enabled         (bool)
└── keys[]
    ├── api_key     (密文；若设置了 DATABASE_SECRET_KEY 则落盘时加密)
    ├── label       (自由文本)
    └── enabled     (bool)
```

一个用户可以没有 API key (仍可登录控制台)，一个 key 也可以没有密码
(纯程序化用户)。`is_admin` 决定访问 `/admin/*` 路由与控制台管理视图的能力。

## 在种子 TOML 中创建用户

```toml
[[users]]
name = "alice"
password = "plain-or-argon2-phc"
enabled = true

[[users.keys]]
api_key = "sk-user-alice-1"
label = "default"
enabled = true

[[users.keys]]
api_key = "sk-user-alice-ci"
label = "ci-runner"
enabled = true
```

`password` 可以是明文 (导入时 GPROXY 用 Argon2 进行哈希)，也可以直接是
Argon2 PHC 字符串 (`$argon2id$…`)，便于从外部系统迁移已经哈希过的凭证。

## 引导 (bootstrap) 管理员

启动时，若种子 TOML 中**没有**任何 `is_admin = true` 且至少有一个启用 key 的用户，
GPROXY 会从以下环境变量创建管理员：

- `GPROXY_ADMIN_USER` (默认 `admin`)
- `GPROXY_ADMIN_PASSWORD` —— 未设置则自动生成并打印一次
- `GPROXY_ADMIN_API_KEY` —— 未设置则自动生成并打印一次

这是"开箱即用"的路径。首次启动时把打印的值保存到密码管理器即可。

## 鉴权入口

| 入口 | 凭证 | 位置 |
| --- | --- | --- |
| LLM 路由 (`/v1/...`、`/v1beta/...`) | 用户 API key | 取决于协议 —— `Authorization: Bearer …`、`x-api-key: …`、`x-goog-api-key: …`。 |
| 控制台 | 用户名 + 密码 | `POST /login` 返回一个 bearer session token，UI 保存并作为 `Authorization: Bearer <session_token>` 发送。 |
| 管理 API | 管理员用户的 API key | `Authorization: Bearer <admin api key>`。 |

控制台与管理 API 共享同一个路由层，区别只在鉴权用户是否 `is_admin = true`。

## 运行时管理用户

数据库运行起来之后，可通过控制台*用户*标签页创建/编辑，或直接调用管理 API：

- `GET    /admin/users` —— 列表
- `POST   /admin/users` —— 创建
- `PATCH  /admin/users/{id}` —— 更新
- `DELETE /admin/users/{id}` —— 删除
- `POST   /admin/users/{id}/keys` —— 新增 key
- `PATCH  /admin/users/{id}/keys/{key_id}` —— 启用/禁用/改 label
- `DELETE /admin/users/{id}/keys/{key_id}` —— 吊销 key

吊销立即生效 —— 下一次携带该 key 的请求将鉴权失败。

## 静态加密

启动时设置 `DATABASE_SECRET_KEY` 会启用数据库加密器：用户密码、API key (以及
供应商凭证) 在落盘前会用 **XChaCha20-Poly1305** 加密。丢失该 key 意味着
无法解密现有记录 —— 请在密码管理器或 KMS 中另行备份。
