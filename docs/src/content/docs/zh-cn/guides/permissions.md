---
title: 权限、限流与配额
description: 如何控制每个用户能调用什么模型、调用多快、花多少钱。
---

三个相互独立的机制决定一个用户能做什么：

1. **权限 (Permissions)** —— *这个用户能调这个模型吗?*
2. **限流 (Rate limits)** —— *能调多快?*
3. **配额 (Quotas)** —— *一共能花多少钱?*

每个请求在调上游之前都会依次检查这三项。任何一项都可以独立拒绝请求，
并返回对应的错误码。

## 权限

权限行是一个 `(user, provider, model_pattern)` 三元组：

```toml
[[permissions]]
user_name = "alice"
provider_name = "openai-main"
model_pattern = "gpt-*"
```

- `model_pattern` 是一个 **glob** (`*`、`?`)，匹配**别名解析之前**的请求模型名。
  写 `chat-*` 就能匹配别名 `chat-default`；写 `gpt-*` 就能匹配真实 id `gpt-4.1-mini`。
- `provider_name` 可省略，表示跨所有供应商授权 (匹配仍沿 permission → rewrite → alias → execute 顺序)。
- 没有任何匹配行的用户会收到 `403 forbidden: model`。

### 文件权限

`[[file_permissions]]` 是一张独立的表，用于授权用户调用某个供应商的
文件相关接口 (upload / retrieve / delete)。它的结构更简单：

```toml
[[file_permissions]]
user_name = "alice"
provider_name = "openai-main"
```

## 限流

限流作用于 `(user, model_pattern)`，可以同时或单独启用三个计数器：

| 字段 | 含义 |
| --- | --- |
| `rpm` | 每**分钟**请求数。 |
| `rpd` | 每**日**请求数。 |
| `total_tokens` | 一段滚动时间窗内的**总 token**。 |

```toml
[[rate_limits]]
user_name = "alice"
model_pattern = "gpt-*"
rpm = 60
rpd = 10000
total_tokens = 200000
```

计数器在内存中维护，由 `RateLimitGC` worker 定期清理过期的窗口，避免进程
积累死计数器。

命中任一计数器的请求会收到 `429 rate_limited`，响应头会指示哪个计数器触发
以及窗口何时重置。

## 配额

配额是每个用户一条美元上限：

```toml
[[quotas]]
user_name = "alice"
quota = 100.0       # 上限 (USD)
cost_used = 0.0     # 已使用成本；由用量记账增长
```

- `UsageSink` worker 在请求完成时把成本写入 `cost_used`。
- `QuotaReconciler` worker 周期性地从真实用量行重算 `cost_used`，
  如果在线计数器发生漂移也会自我修正。
- 一旦请求预期成本会让 `cost_used` 超过 `quota`，会返回 `402 quota_exceeded`。

配额只是**计费**上限，不是安全上限 —— 它只对配置了价格的模型/别名起作用。
没定价的模型对 `cost_used` 贡献为 0。

## 执行顺序

对一个请求来说，检查顺序是：

1. **鉴权** —— 把 API key 解析为用户。
2. **权限** —— 是否有匹配的 `[[permissions]]` 行？没有 → `403 forbidden: model`。
3. **配额** —— `cost_used < quota`？否则 → `402 quota_exceeded`。
4. **限流** —— 计数器是否仍在上限以内？否则 → `429 rate_limited`。
5. **执行** —— 进入 `rewrite → alias → execute`。

因此权限拒绝永远优先于限流拒绝，而限流拒绝永远优先于上游执行错误。

## 运行时管理

上述三类配置都可以在控制台的*用户* → *权限 / 限流 / 配额*工作区中
实时编辑，也可以通过 `/admin/permissions`、`/admin/rate_limits`、`/admin/quotas`
等管理 API 修改。变更在下一次请求时生效，不需要 reload。
