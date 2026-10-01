---
title: "边缘部署（Cloudflare Workers）"
description: "配置 Workers、D1、secrets 与控制台资源，了解当前部署限制。"
---

Workers 使用与原生服务相同的 HTTP 路由，数据库和文件存储需要使用远程后端。控制台作为 Workers Assets 部署，不嵌入 WASM。

## 当前限制

- Responses WebSocket、Realtime 和渠道 service socket 复用原生部署的路由、鉴权与限制。
- 默认支持 D1；libSQL 与 S3/R2 需要在构建时启用对应 feature。
- 不支持本地 SQLite、TCP 数据库、本地文件目录或进程内缓存。当前 Worker 装配使用 `store` 缓存，不提供 Redis 客户端。
- 首次启动通过 `GPROXY_ADMIN_PASSWORD` Secret 创建管理员，默认用户名为 `admin`；可用 `GPROXY_ADMIN_USER` 修改。Workers 不显示 Application 设置向导。

## 使用发布包

下载所选版本的 `gproxy-edge-cloudflare.zip`，解压并进入 `cloudflare` 目录。包中包含构建好的 Worker、控制台资源和 `wrangler.toml`。

安装依赖并创建 D1 数据库：

```sh
pnpm install
pnpm exec wrangler d1 create gproxy
```

将返回的数据库 ID 填入 `wrangler.toml` 的 `database_id`，保留 binding 名 `DB`。

```toml
[[d1_databases]]
binding = "DB"
database_name = "gproxy"
database_id = "replace-with-your-database-id"
```

保存一份 32 字节主密钥（64 位十六进制或 base64），通过 secret 输入：

```sh
pnpm exec wrangler secret put GPROXY_MASTER_KEY
pnpm exec wrangler secret put GPROXY_ADMIN_PASSWORD
pnpm exec wrangler deploy --dry-run
pnpm exec wrangler deploy
```

主密钥应长期保留，后续部署继续使用同一值。未配置时凭证以明文保存。

发布前用 dry run 检查当前产物体积与平台限制，不要使用旧版本的 WASM 体积估算部署是否可行。

## 配置

`GPROXY_CONFIG` 是 JSON 配置；具名 secrets 覆盖对应字段。未指定时使用 `DB` D1 binding 和数据库缓存。

```toml
[vars]
GPROXY_CONFIG = """
{
  "store": { "kind": "d1", "binding": "DB" },
  "cache": { "kind": "store" },
  "public_base_url": "https://gproxy.example.workers.dev"
}
"""
```

| Secret | 用途 |
| --- | --- |
| `GPROXY_ADMIN_PASSWORD` | 空库初始管理员密码，至少 8 个字符；只在首次初始化使用 |
| `GPROXY_MASTER_KEY` | 加密凭证的主密钥 |
| `GPROXY_LIBSQL_TOKEN` | libSQL / Turso 令牌，仅启用该后端时需要 |
| `GPROXY_S3_ACCESS_KEY_ID` | S3 / R2 访问标识 |
| `GPROXY_S3_SECRET_ACCESS_KEY` | S3 / R2 访问密钥 |

需要发布可下载文件或保存词表时，配置 S3/R2 文件存储并启用 `s3` feature。需要公开文件链接时设置 `public_base_url`。

## 控制台与路由

发布包包含以下 Assets 设置。除控制台路径外，请求先交给 Worker，确保自定义供应商前缀也能正确转发。

```toml
[assets]
directory = "./public"
binding = "ASSETS"
not_found_handling = "single-page-application"
run_worker_first = ["/*", "!/", "!/console", "!/console/*"]
```

首次请求完成初始化后，访问 `/console/`，使用 `admin` 和配置的初始密码登录。然后在控制台创建网关 API Key。已有任何用户时，初始化会跳过，不会重置密码；可删除初始密码 Secret。空库未提供密码或密码不符合要求时，启动会报错。`/healthz` 仅用于健康检查，不能代替登录和上游请求验证。

## 数据库与配置同步

每个 isolate 首次装配时执行 Store 表结构同步，然后读取配置与身份数据。当前部署包不依赖单独的 Wrangler SQL migration 文件，不能把 `wrangler d1 migrations apply` 当作 GPROXY 初始化步骤。

每次请求前检查配置版本并刷新已加载的状态。修改数据库配置后，其他 isolate 在后续请求中同步。升级前应备份数据库，并检查首次请求的日志。

## 从源码构建

从仓库根目录执行：

```sh
rustup target add wasm32-unknown-unknown
cargo install worker-build
pnpm --dir console install --frozen-lockfile
pnpm --dir deploy/cloudflare install
pnpm --dir deploy/cloudflare build
pnpm --dir deploy/cloudflare check
```

部署前仍需在 Cloudflare 上验证数据库、身份认证和至少一次上游调用。源码编译与 Wrangler dry run 不等于线上功能验证。
