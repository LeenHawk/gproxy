---
title: Console
description: 使用 v2 React console 管理 provider、route、rule、identity、settings、usage、logs 和 portal 工作流。
---

v2 console 是 `console/` 下的 React app。native binary 从 `/console` 提供构建后的静态资源；本地开发时 Vite dev server 代理后端路由。

Console 调用的 admin/user API 与 edge runtime 使用的是同一套 dispatcher：

| Surface | 用途 |
| --- | --- |
| `/admin/*` | 管理控制面、观测、settings、update 和运维端点。 |
| `/user/*` | 用户门户中的 account、key、limit、audit 和 usage。 |
| `/healthz`、`/version`、`/metrics` | 需要 admin 的 ops endpoints。 |
| `/v1/*`、`/v1beta/*`、`/{provider}/v1/*` | 网关流量，使用用户 API key 认证。 |

## 本地开发

本地 HTTP 开发时，后端需要允许 insecure cookies：

```bash
GPROXY_INSECURE_COOKIES=1 cargo run --features full
```

然后启动 console：

```bash
cd console
pnpm dev
```

`console/vite.config.ts` 会把 `/admin`、`/healthz`、`/version`、`/metrics`
代理到 `http://127.0.0.1:8787`，并改写 origin 以通过 CSRF 检查。生产 native
二进制会从同一 origin 提供构建后的 console 和 `/user/*` portal API。

## 主要管理区域

| 区域 | 管理内容 |
| --- | --- |
| Providers | Provider、credential、TLS preset、provider model、上游模型拉取、routing rules、provider rule-set attachment。 |
| Routes | Aggregated route name、alias、route member、strategy 和 route settings。 |
| Rules | 可复用 rule set 和具体 process rule。 |
| Users | Org、team、user、key、permission、rate limit 和 quota。 |
| Usage | Usage row、rollup、downstream/upstream request log、audit log、credential status。 |
| Settings | Instance settings、proxy、logging、usage、tokenizer download、retention、update channel。 |
| Update | 支持 native self-update 的运行时状态。 |

## Build and Embed

native 生产构建前运行：

```bash
cd console
pnpm build
```

Build 会运行 TypeScript、Vite 和 `scripts/sync-to-embed.mjs`，把 `console/dist/` 复制到 `assets/console/`，供 `rust-embed` 编译进 binary。如果没有执行这一步，后端仍可编译，但只会提供 placeholder embed directory。

## 配置哲学

Console 应尽量承载 provider-specific policy。对 transform 行为来说，这意味着 wizard 和 template 应生成 generic 或已有 rule config。后端 process engine 保持宽松，并以 Operation 为组织中心。
