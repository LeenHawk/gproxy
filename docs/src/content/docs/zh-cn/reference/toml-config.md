---
title: TOML 配置
description: v1 TOML seed 文件的兼容说明，以及 v2 支持的 JSON import/export 格式。
---

gproxy v2 的 native serve 路径不读取 `gproxy.toml` seed 文件。v1 的
`GPROXY_CONFIG` / TOML bootstrap 模型已经被替换为：

- 启动时的 CLI 参数和环境变量，用于进程级设置；
- 持久化层中的 control-plane 行，用于运行时配置；
- JSON bundle import/export，用于可复现引导和迁移。

本页保留 `toml-config` slug，是为了兼容 v1 文档入口；v2 当前支持的是
JSON，不是 TOML。

## 支持的 import/export 命令

导入 bundle 到当前配置的持久化后端，然后退出：

```bash
./gproxy \
  --persistence db \
  --dsn 'sqlite:///var/lib/gproxy/gproxy.db?mode=rwc' \
  import --in ./import.json
```

导出 control-plane 配置，然后退出：

```bash
./gproxy \
  --persistence db \
  --dsn 'sqlite:///var/lib/gproxy/gproxy.db?mode=rwc' \
  export --out ./export.json
```

导出文件包含明文 provider credential 和用户 API key。Unix 下会以仅 owner
可读写权限写入，并使用同目录临时文件加原子 rename，但它仍然是 secret 文件。

## 首启导入 hook

正常 serve 路径中，`GPROXY_IMPORT_FILE` 只会在 store 为空时导入：

```bash
GPROXY_IMPORT_FILE=/etc/gproxy/import.json ./gproxy
```

该 hook 在 admin bootstrap 之前运行。bundle 中已有 admin 用户时，不会再创建随机 admin。store 中已有任意 provider 或 user 后，该 hook 会跳过。

## Bundle 结构

v2 bundle 使用 `schema_version: 1`，其余字段是持久化 input record 数组。
跨记录引用使用原始数字 id，因此需要引用其它记录的 bundle 必须显式固定 id。

```json
{
  "schema_version": 1,
  "orgs": [
    { "id": 1, "name": "default", "enabled": true, "description": null }
  ],
  "users": [
    {
      "id": 1,
      "name": "admin",
      "org_id": 1,
      "team_id": null,
      "password": "$argon2id$...",
      "enabled": true,
      "is_admin": true
    }
  ],
  "user_keys": [
    {
      "id": 1,
      "user_id": 1,
      "api_key": "sk-replace-with-a-long-random-key",
      "label": "bootstrap",
      "enabled": true
    }
  ],
  "providers": [
    {
      "id": 1,
      "name": "openai-main",
      "channel": "openai",
      "label": null,
      "settings_json": { "base_url": "https://api.openai.com" },
      "credential_strategy": "round_robin",
      "proxy_url": null,
      "tls_fingerprint": null,
      "enabled": true
    }
  ],
  "credentials": [
    {
      "id": 1,
      "provider_id": 1,
      "label": "primary",
      "kind": "api_key",
      "secret_json": { "api_key": "sk-provider-key" },
      "weight": 100,
      "rpm_limit": null,
      "tpm_limit": null,
      "proxy_url": null,
      "tls_fingerprint": null,
      "enabled": true
    }
  ],
  "provider_models": [
    {
      "id": 1,
      "provider_id": 1,
      "model_id": "gpt-4.1-mini",
      "display_name": "GPT-4.1 mini",
      "pricing_json": { "input": "0.40", "output": "1.60" },
      "variants_json": null,
      "enabled": true
    }
  ],
  "routes": [
    {
      "id": 1,
      "name": "main",
      "strategy": "failover",
      "enabled": true,
      "description": null,
      "settings_json": null
    }
  ],
  "route_members": [
    {
      "id": 1,
      "route_id": 1,
      "provider_id": 1,
      "upstream_model_id": "gpt-4.1-mini",
      "weight": 100,
      "tier": 0,
      "enabled": true
    }
  ],
  "aliases": [
    { "id": 1, "alias": "default-chat", "route_id": 1 }
  ]
}
```

## 支持的顶层数组

| 数组 | 用途 |
| --- | --- |
| `orgs`, `teams`, `users`, `user_keys` | 身份、admin 登录和 API key。导入的 API key 会生成 digest 供鉴权查找，并按当前 cipher 存储。 |
| `route_permissions`, `rate_limits`, `quotas` | org/team/user 作用域的访问权限、token limit 和费用 quota。 |
| `providers`, `credentials`, `provider_models` | 上游 provider、密封 credential、暴露模型、可选 pricing 和 variants。 |
| `routes`, `route_members`, `aliases` | 逻辑模型名、后端池和 alias。 |
| `routing_rules` | 每个 provider 的 transform dispatch 行。通过 admin API 创建 provider 会自动 seed 默认路由；原始 bundle import 只导入你提供的行。 |
| `rule_sets`, `rules`, `provider_rule_sets` | 可复用的请求/响应变更规则集，以及 provider 绑定。 |
| `instance_settings` | 单例实例行为，例如 retention 和 tokenizer download 设置。 |

## 运行时配置来源

导入后，持久化后端就是 source of truth。修改磁盘上的 JSON 文件不会改变正在运行的服务，除非你再次执行 import 命令，或在空 store 首启时通过 `GPROXY_IMPORT_FILE` 导入。日常操作应使用 console 或 admin API。

## 从 v1 变化

| v1 概念 | v2 替代 |
| --- | --- |
| `GPROXY_CONFIG=gproxy.toml` | 当前 v2 没有等价项。进程设置用环境变量，control-plane seed 用 JSON import/export。 |
| TOML provider/model/user 数组 | 与 v2 持久化 input record 对应的 JSON bundle 数组。 |
| 修改 TOML 后重新读取 | 不支持。运行时行通过 admin API/console 编辑，并通过 snapshot invalidation 生效。 |
| `DATABASE_SECRET_KEY` 运行时加密 | v2 使用 `GPROXY_MASTER_KEY` 密封 secret；`DATABASE_SECRET_KEY` 只用于迁移时读取加密的 v1 数据。 |
