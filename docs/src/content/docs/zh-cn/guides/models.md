---
title: "模型与路由"
description: 客户端的模型名如何解析到一个 Provider 和一把凭证：四种形式、路由与成员、namespace 与模型目录。
---

客户端的模型名很少就是上游模型 id。v4 在**进引擎之前**解析它，因此引擎只见已选定的目标。

```text
请求里的 model
  → 第一条命中的形式：路由名 · 渠道/模型 · Provider 名/模型
  → 候选按渠道、允许的 Provider、允许的凭证收窄
  → 按 (tier, 健康度, 权重倒序, 稳定 id) 排序
  → 首段按路由策略均衡
  → 每次尝试得到一个 (Provider, 凭证, 上游模型)
```

直接使用路由名称作为客户端模型名。需要修改请求参数时，使用[重写规则](/zh-cn/guides/rules/)。

## 四种形式

按第一条命中的规则解析。

| 名字 | 解析为 | 尝试预算 |
| --- | --- | --- |
| 没有模型 | 全部启用的 Provider，无上游模型 | `settings.max_attempts` |
| **模型路由名称** | 该路由启用的成员 | 路由自己的 |
| `渠道/模型` | 该渠道的 Provider，优先目录里列了该模型的 | `settings.max_attempts` |
| `Provider 名/模型` | 那一个 Provider | `settings.max_attempts` |
| 其他 | `404 unknown_model` | — |

路由名**精确匹配且先于**前缀形式。管理接口会拒绝首段与已注册渠道或 Provider 前缀冲突的名称。

前缀形式内部，**渠道 id 胜过同名 Provider**。渠道 id 由构建固定、改不掉；Provider 名随时
可以改。反过来更糟：把某个 Provider 命名为 `codex`，全部 `codex/*` 流量就再也到不了
`codex` 渠道，而且没有任何绕开的办法。

## 路由

路由名称就是客户端请求的模型名，下含供应商／上游模型成员、均衡策略和尝试预算。
创建 `main` 路由后，客户端直接请求 `model: "main"`。

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/routes \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"name":"main","strategy":"round_robin","maxAttempts":6}'
```

```json
{"id":"33a88261f571347c7f0408c3bd2e2164","name":"main","strategy":"round_robin",
 "maxAttempts":6,"enabled":true}
```

| 字段 | 含义 |
| --- | --- |
| `name` | 全局唯一，客户端在 `model` 中直接填写此名称。 |
| `sessionAffinity` | 会话优先复用成功的供应商／模型目标，默认关闭；内部凭证选择仍由供应商负责。 |
| `strategy` | `round_robin`、`weighted` 或 `failover`。 |
| `maxAttempts` | 含首次调用在内的总尝试预算。执行时以 `settings.maxAttempts`（默认 6）为硬上限。 |

## 成员

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/route-members \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"routeId":"…","providerId":"…","upstreamModel":"gpt-4o-mini",
       "tier":0,"weight":100}'
```

| 字段 | 含义 |
| --- | --- |
| `providerId`、`upstreamModel` | 这个成员把流量发到哪。模型名是一个显式字符串，不是目录外键。 |
| `tier` | 越小越优先。tier 0 耗尽之前 tier 1 拿不到任何流量。 |
| `weight` | 正数，默认 100。层内分流，同时决定故障转移候选的顺序。 |
| `enabled` | 被禁用的成员退出计划。 |

成员不指定凭证。选中的 Provider 内部由哪把凭证承接是引擎另做的决定，而且它会在计划移动到
下一个成员之前，先在该 Provider 的凭证之间做转移。

### 顺序怎么定

候选按 `(tier, 健康度, 权重倒序, 稳定 id)` 排序。

**tier 是硬偏好。** 只有首段——与第一个候选 tier *和*健康度都相同的那一串——参与均衡，
然后才按策略处理：

| 策略 | 对首段的作用 |
| --- | --- |
| `round_robin` | 用一个按路由的计数器轮转它 |
| `weighted` | 把平滑加权选中的提到队首 |
| `failover` | 原样保留：排序结果*就是*答案 |

轮转是计数器而不是随机抽取，因此 wasm 与原生行为一致，一次序列可复现。

禁用、已退役和已死的凭证直接去掉。被封禁的凭证在其 Provider 还有别的可用凭证时去掉；
若**全部**被封禁则保留该 Provider 并排在所有健康 Provider 之后。限流是最后手段，不是故障。

解析成功但无处可发，与"名字不认识"是两回事——那是配置问题，不是未知模型。

## 路由名称

直接在路由上创建和修改客户端使用的模型名，不需要额外的公开名称映射。

### namespace

带 `/` 的名字在运行期派生出一个 namespace：创建 `acme/fast` 路由 就让 `acme` 成为一个挂载点，
而 `/acme/v1/chat/completions` 配 `{"model":"fast"}` 解析的是 `acme/fast`。

```sh
curl -s http://127.0.0.1:8787/acme/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"hi"}]}'
```

namespace 是一个**名字索引**，不是被存储的分组，也不是归属范围。它不创建任何东西，也不
拥有任何东西。

### 被保留的第一段

为了避免 namespace 与前缀路由歧义，首段是已注册渠道 id 或现有 Provider 名的路由名会在写入时被拒绝：

```json
{"error":{"code":"invalid_request","message":"invalid request: `codex/` is reserved:
 a first segment naming a channel or a provider already means `channel/model` or
 `provider/model` narrowing, so `codex/fast` could never reach its route"}}
```

其他都行。`coding/fast` 是个完全好用的公开名。

## 模型目录

两张表，而且路由都不需要它们。

**`provider_models`** 记录某个 Provider 承接哪些上游名字：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/provider-models \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","upstreamName":"gpt-4o-mini"}'
```

```json
{"id":"f32df0bb6378c03e70f7c3aa6315a3b8","providerId":"5a45fd807be0…",
 "upstreamName":"gpt-4o-mini","modelId":null,"metadata":{},"enabled":true}
```

`渠道/模型` 形式在该渠道的多个 Provider 中做选择时会优先它，模型发现也写在这里。
**`models`** 是一行 `provider_models` 可以指向的全局目录：一个名字、它的元数据，以及
token 估算该用的词表。

路由可以匹配两张表里都没有的名字。路由成员把上游模型写成一个普通字符串，所以目录行是
文档与定价材料，不是前置条件。

### 从上游填充

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/models/discover \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…"}'
```

发现用 Provider **自己的方言**提问，因此什么都不转换，名字就是上游的。每个答案带着这个
Provider 是否已有该行、以及内置目录能不能给它定价。
`POST /admin/api/models/discover/apply` 插入你点名的那些，已有的跳过，所以发现应用两次和
一次结果相同。

它和 `POST /admin/api/models/test` 都花真凭证、写真用量行。见
[两个探针](/zh-cn/guides/providers/#两个探针)。

内置目录——本次发布知道的模型名、上下文窗口与默认价格——是生成那份资产时的快照，不是一份
实时目录：

```sh
curl -s http://127.0.0.1:8787/admin/api/default-model-catalog -H "Authorization: Bearer $GPROXY_KEY"
curl -s -X POST http://127.0.0.1:8787/admin/api/default-model-catalog/apply-prices \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","modelIds":["gpt-4o-mini"],"overwrite":false}'
```

`overwrite: false` 正是让重复应用安全的东西：运维者改过的规则保留它的改动，并被报告为跳过。

## 调用方看到什么

`GET /v1/models` 返回已配置的模型清单，包括路由名和
`Provider 名/模型`，只列出调用方有权使用的名字，不会轮流返回某一家上游的目录。
新增或导入供应商模型后，它会出现在这份清单中，前缀使用配置的供应商路由名。
`渠道/模型` 仍可用于调用，但不会再自动添加为另一条记录。

两种调用方式等价：API 地址用 `/供应商A/v1`、模型填 `gpt-5`；
或者地址用 `/v1`、模型填 `供应商A/gpt-5`。
`GET /供应商A/v1/models` 只向供应商 A 查询，保留上游模型原名。

用户面的清单还会显示没有权限的名字，并用 `permitted` 标注：

```sh
curl -s http://127.0.0.1:8787/portal/api/models -H "Authorization: Bearer $GPROXY_KEY"
```

```json
[{"name":"provider-A/gpt-4o-mini","providerCount":1,"channelIds":["custom"],"permitted":true},
 {"name":"fast","providerCount":1,"channelIds":["custom"],"permitted":true}]
```

调用方规则触达不到的名字仍然在清单里，只是 `permitted: false`。v3 会丢掉这样的行，v4 不
——因为一份会静默省略的清单会让"这个模型 404 了"和"你没有权限用这个模型"变成同一个观察，
而且没什么要保护的：公开名本来就是运维者对外发布的实例配置。

被扣下的是名字背后的 Provider id：答案只报一个数量和一个渠道，说明一个名字有多冗余，
却不点破机器。`Provider 名/模型` 使用供应商的显示名称；供应商改名后，对应的模型前缀和 API 地址也随之改变。

### 全局模型与默认元数据

Console 的“模型”（`/console/model-catalog`）展示数据库中保存的模型，与权限选择框统一使用 `/admin/api/models`。数据库为空时返回空列表。“加载默认模型”将选中的内置模型写入数据库，保留已有模型，不修改价格规则；也可以手动新增或从 OpenRouter 导入。删除模型后，读取时不会从内置表自动补回。页面展示已保存的能力和上限、供应商关联，以及可匹配到的内置参考价格。

默认模型表使用 `claude-sonnet-4` 这样的裸模型名，去掉 `anthropic/` 等来源前缀；来源 URL 和原始报价仍保留。带前缀的上游名称仍可按末段模型名匹配。默认价格选择最长的匹配模型片段，供应商导入价格时按其实际上游名称保存规则。价格继续在全局模型和供应商模型的现有弹窗中编辑。

渠道拉取时，优先按完整模型 ID 匹配，再尝试唯一的末段名称；有歧义则不补全。合并顺序为内置默认值 → 上游非空字段 → 本地覆盖值。Console 导入已有渠道模型时只补缺失字段，已有设置不被覆盖。元数据是导入时复制的快照，编辑总表不会自动修改已经导入的渠道模型。

默认价格是 OpenRouter 报价快照，不代表所有渠道的实际合同。应用默认价格会生成全局计费规则，已有同模式规则不覆盖；价格编辑器支持费率、上下文分段及服务等级。渠道专属规则优先于全局规则。

内置数据通过 `node scripts/update-openrouter-model-catalog.mjs` 更新；可用 `--input response.json` 离线生成。公开模型接口无需 Key；可选凭证只从 `OPENROUTER_API_KEY` 环境变量读取。生成器保留原始 `source_pricing` 作为核查材料，只把能确认单位的字段写入计费结构。单位换算后的计费价格自动舍入到 9 位小数，采用就近舍入、中点取偶；导入默认价格时也使用相同规则。缺失或动态价格不应当解释为免费。额外官网价格应先确认模型版本、地区、单位和阈值，再加入，不以猜测补齐。
