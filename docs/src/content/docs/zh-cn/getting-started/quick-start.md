---
title: 快速开始
description: "从一个构建好的二进制到一个被计量的请求：启动 gproxy，加一个 Provider 和一把凭证，公开一个模型名，然后调用它。"
---

本页把一个全新实例带到它的第一个成功请求。它假设你已有一个来自
[安装](/zh-cn/getting-started/installation/)的二进制，并且全程使用管理 API——因为源码
检出里没有 console bundle。

下面每条命令都对着一个临时数据目录真实跑过，输出就是它回来的东西。

## 1. 启动实例

```sh
./target/release/gproxy serve --data-dir ./data --port 7070
```

首次启动把管理员和一把网关 API key 只打印一次到标准输出：

```text
GPROXY first-run administrator (shown once)
  user:     admin
  password: zfpub7rfV2PO2PAbqazAVt-yC_yfwLrt
  api key:  sk-56sjXy3JADsZjYl1g3QnzzTNH-NBmzPNyUKf22qgVzI
Save these before closing this terminal; they are not stored in a form
this instance can show you again.
```

这把 key 既是网关 key 也是管理员的 key，因此它同时打开 `/admin/api` 和 `/v1`。本页余下
部分把它放在一个 shell 变量里：

```sh
export GPROXY_KEY='sk-56sjXy3JADsZjYl1g3QnzzTNH-NBmzPNyUKf22qgVzI'
```

确认它起来了。`/healthz` 不需要认证，也不碰数据库和 cache，因此负载均衡器轮询它不会
自己变成压垮它的那份负载：

```sh
curl -s http://127.0.0.1:7070/healthz
```

```json
{"revision":2,"status":"ok"}
```

## 2. 看这个二进制有哪些渠道

渠道是某一族上游的适配器，只有编译进去的才存在。这份清单来自二进制，不是数据库：

```sh
curl -s http://127.0.0.1:7070/admin/api/channels \
  -H "Authorization: Bearer $GPROXY_KEY"
```

默认构建回答全部 25 个：`aistudio`、`antigravity`、`aws_bedrock`、`azure`、`claudeapi`、
`claudecode`、`claudeweb`、`cline`、`codex`、`copilotcli`、`custom`、`dashscope`、
`deepseek`、`devin`、`geminicli`、`grokbuild`、`kimi`、`kiro`、`openai`、`opencode`、
`openrouter`、`vertex`、`vertexexpress`、`workbuddy`、`xai`。

每一项都带着它的登录方式、能力，以及一个 Provider 表单该渲染哪些 `config` 键——这就是
一个管理 UI 渲染表单所需的全部。

## 3. 添加一个 Provider

Provider 是某个渠道上的一条已保存连接。`custom` 是通用的 API-key 渠道：任何原生讲
OpenAI、Claude 或 Gemini 的端点。

```sh
curl -s -X POST http://127.0.0.1:7070/admin/api/providers \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{
    "name": "openai-main",
    "channel": "custom",
    "baseUrl": "https://api.openai.com",
    "config": { "dialects": ["openai_chat", "openai"] }
  }'
```

```json
{"id":"5a45fd807be02516a1626eedd528859d","name":"openai-main","channel":"custom",
 "baseUrl":"https://api.openai.com","connectionProfileId":null,
 "config":{"dialects":["openai_chat","openai"]},"enabled":true,
 "createdAtMs":1789981558211}
```

`name` 是运维者取的标签，同时也是 **Provider 挂载点**：
`/openai-main/v1/chat/completions` 只会触达这个 Provider。`dialects` 告诉 `custom` 渠道
这个端点讲哪些线格式——把你想让它承接的都点上名；方言不在列表里的操作没有转换能到达它。

记下 id：

```sh
export PROVIDER=5a45fd807be02516a1626eedd528859d
```

## 4. 添加一把凭证

```sh
curl -s -X POST http://127.0.0.1:7070/admin/api/credentials \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d "{\"providerId\":\"$PROVIDER\",\"label\":\"main key\",
       \"authKind\":\"api_key\",\"secret\":{\"api_key\":\"sk-…\"}}"
```

```json
{"id":"b8aad67f1af2cdb265218b8d1686a2ef","providerId":"5a45fd807be02516a1626eedd528859d",
 "organizationId":null,"teamId":null,"userId":null,"label":"main key",
 "authKind":"api_key","hasSecret":true,"version":0,"connectionProfileId":null,
 "metadata":{},"expiresAtMs":null,"status":"active","statusReason":null,"enabled":true}
```

密钥从不出现在列表里——只有 `hasSecret`，外加一个单独的、被审计的
`POST /admin/api/credentials/{id}/reveal`。账号池里有几把就加几把；引擎会在它们之间轮转，
并在计划移动到下一个成员之前先在该 Provider 内部做转移。

对于靠登录而不是贴 key 的渠道（`codex`、`claudecode`、`kiro`…），凭证来自
[登录流程](/zh-cn/guides/providers/#通过登录获取凭证)。

## 5. 通过 Provider 挂载点调用

现在已经可以发请求了。Provider 挂载点不需要路由，也不需要公开名：

```sh
curl -s http://127.0.0.1:7070/openai-main/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"Say hello."}]}'
```

在聚合挂载点上，`provider/model` 形式做的是同一件事：

```sh
curl -s http://127.0.0.1:7070/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"openai-main/gpt-4o-mini","messages":[{"role":"user","content":"Say hello."}]}'
```

## 6. 给它一个公开名

路由是一组带 tier 与 weight 的成员；公开模型名是指向某条路由的对外名字。两者合起来，
就是让客户端不再念你的基础设施。

```sh
curl -s -X POST http://127.0.0.1:7070/admin/api/routes \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"name":"main"}'
```

```json
{"id":"33a88261f571347c7f0408c3bd2e2164","name":"main","strategy":"round_robin",
 "maxAttempts":6,"enabled":true}
```

```sh
export ROUTE=33a88261f571347c7f0408c3bd2e2164

curl -s -X POST http://127.0.0.1:7070/admin/api/route-members \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d "{\"routeId\":\"$ROUTE\",\"providerId\":\"$PROVIDER\",
       \"upstreamModel\":\"gpt-4o-mini\"}"

curl -s -X POST http://127.0.0.1:7070/admin/api/exposed-models \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d "{\"routeId\":\"$ROUTE\",\"name\":\"fast\"}"
```

```json
{"id":"fe91bb79f182e5becd899a056865c707","routeId":"33a88261f571347c7f0408c3bd2e2164",
 "providerId":"5a45fd807be02516a1626eedd528859d","upstreamModel":"gpt-4o-mini",
 "tier":0,"weight":100,"enabled":true}
{"id":"c524d5e47279e2eb3fa86b2143e4f755","name":"fast",
 "routeId":"33a88261f571347c7f0408c3bd2e2164","enabled":true}
```

在另一个 Provider 上加一个 `tier: 1` 的成员，就是一个故障转移目标。两个 `tier: 0` 的成员
按权重分流。

每一次写入都是一个事务连同配置 revision 自增，而且实例先重载、再通知同伴，所以新名字
在下一个请求上就能用。

## 7. 发送请求

```sh
curl -s http://127.0.0.1:7070/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"Say hello."}]}'
```

## 8. 看看花了多少

```sh
curl -s http://127.0.0.1:7070/portal/api/usage \
  -H "Authorization: Bearer $GPROXY_KEY"
```

```json
{"fromMs":null,"toMs":null,"summary":{"requests":2,"inputTokens":4,
 "outputTokens":318,"cachedInputTokens":0,"cacheCreationTokens":0,
 "reasoningTokens":0,"cost":"0","currency":null,"truncated":false,"scanned":2},
 "groups":[],"trend":[]}
```

`cost` 是 `0`，因为还没有价格规则覆盖这个模型。请求依然被结算、依然被记录，并带上
`unpriced = true` 维度——运维者要的是"有个模型在被白嫖"这个信号，而不是一个拒绝。
见[价格与分层](/zh-cn/reference/pricing/)。

## 下一步

- [发送第一个请求](/zh-cn/getting-started/first-request/)——同一个调用在每种格式下的写法、
  流式，以及三个挂载点。
- [Provider 与凭证](/zh-cn/guides/providers/)——凭证池、登录流程、健康。
- [模型、路由与公开名称](/zh-cn/guides/models/)——tier、weight 与 namespace。
- [CLI 客户端](/zh-cn/guides/cli-clients/)——把 Codex CLI 和 Claude Code 指向网关。
