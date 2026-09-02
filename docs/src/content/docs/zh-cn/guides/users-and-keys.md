---
title: "用户与 API 密钥"
description: "组织、团队、用户与 API 密钥；管理员账户、用户门户、管理 API 访问，以及密钥如何随请求发送"
---

网关流量通过用户 API 密钥认证为某个用户。控制台与门户使用用户名和密码登录，
并由服务端会话维持。

```text
Organization
`-- Team
    `-- User
        |-- password    optional; console or portal login
        |-- is_admin    grants /admin and the admin API
        `-- API keys    gateway traffic
```

用户可以属于某个团队、直接属于某个组织，或两者都不属于。每一级都有启用标记。
权限、限流和配额可以挂在任一级并向下继承，见
[权限、限流与配额](/zh-cn/guides/permissions/)。

## 管理员

管理员是一个带密码和 `is_admin` 的普通用户。在全新的存储上，控制台会显示
初始化页面来创建它。也可以通过环境变量预置：

```sh
GPROXY_ADMIN_USER=admin            # default
GPROXY_ADMIN_PASSWORD=<password>   # required for the bootstrap options below
GPROXY_BOOTSTRAP_ADMIN_API_KEY=sk-...   # optional; a sealed key is generated otherwise
GPROXY_BOOTSTRAP_CHANNELS=codex,claudecode   # optional; creates one provider per id, named after it
```

这些变量不会修改已有账户，唯一例外是管理员密码：只要设置了，每次启动都会
应用。

管理员会话使用 `gproxy_admin_session` Cookie，有效期 12 小时，限定在
`/admin` 路径。`/admin/api/` 下的管理 API 也接受
`Authorization: Bearer <key>`，只要该密钥属于一个已启用且带 `is_admin` 的用户；
没有单独的管理员密钥类型。每一次管理写操作和每一次密文显示都会记入管理操作
审计。

## 创建用户

在 **身份** 中先创建组织，可选地在其中创建团队，然后创建用户。用户有名称、
可选的组织和团队、启用标记、管理员角色和可选密码。编辑时密码留空即保持不变。

## 签发密钥

密钥为单个用户创建，包含：

| 字段 | 含义 |
| --- | --- |
| 前缀 | `sk`（标准）或 `at`（Codex 风格 access token）。摘要忽略前缀，因此两种写法指向同一把密钥。 |
| 标签 | 可选。 |
| 过期时间 | 可选，必须是将来的时间；过期密钥会被拒绝。 |
| 启用 | 禁用的密钥会被拒绝。 |

完整密钥形如 `sk-gp-<43 个 URL 安全字符>`，只在创建时显示一次。列表显示前
12 个字符。**显示密钥** 会再次返回完整密钥，前提是其密封材料已存储；这是一
项会被审计的操作（`user_key.reveal`）。仅以摘要导入的密钥无法显示。

密钥按摘要查找。摘要算法带版本号，可以在不作废已存密钥的前提下更换；版本 1
是对密钥载荷的 SHA-256。以本二进制不支持的版本存储的密钥会被忽略。

没有原地轮换：创建新密钥，迁移客户端，然后禁用或删除旧密钥。

## 发送密钥

准入按以下顺序读取第一个存在的请求头：

| 请求头 | 典型客户端 |
| --- | --- |
| `Authorization: Bearer <key>` | OpenAI SDK、Codex CLI、Claude Code |
| `x-api-key: <key>` | Anthropic SDK |
| `x-goog-api-key: <key>` | Google GenAI SDK |

不接受 Gemini 的 `?key=` Query 参数，请使用请求头。任何请求头都可用于任何
路径：请求头只携带密钥，不决定协议。

## 用户门户

非管理员用户在 `/portal` 使用管理员设置的用户名和密码登录。会话 Cookie
`gproxy_portal_session` 有效期 12 小时。登录尝试按来源地址和用户名分别限流。

门户显示：

- **连接**：Base URL、账户可调用的模型，以及 curl、OpenAI / Anthropic /
  Google SDK、Codex CLI 和 Claude Code 的可复制片段（见
  [CLI 客户端](/zh-cn/guides/cli-clients/)）；
- **配额窗口**：应用于该账户的支出进度条；
- **用量与成本**：1、7 或 30 天范围；
- **最近结算请求**：管理员在门户设置中开启后可见（绝不显示请求体）；
- **API 密钥**：以 `sk` 或 `at` 前缀创建、撤销；以及修改密码表单。

门户创建的密钥在创建后无法再次显示。

## OAuth 签发的密钥

当 Codex CLI 通过 GPROXY 内置的 OAuth 签发端登录时，批准登录的门户用户会得
到一把标签为 `Codex OAuth`、前缀为 `at-gp-oauth-` 的密钥，GPROXY 签发的
access token 会映射回这把密钥。这些请求与其他请求一样计入该用户。见
[CLI 客户端](/zh-cn/guides/cli-clients/)。
