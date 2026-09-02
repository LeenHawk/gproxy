---
title: "模型、路由与别名"
description: "客户端模型名如何经由别名、变体后缀和路由解析到 Provider 凭证，以及模型列表如何生成"
---

客户端的模型名很少就是上游模型 id。在聚合模式下，请求中的 `model` 在选出
凭证之前按固定顺序解析：

```text
request model
  -> alias (global, then provider-scoped when the provider is known)
  -> variant suffix (thinking level, service tier, ...)
  -> exposed model -> route -> members by tier and weight
  -> provider credential
```

在控制台中，路由称为 **负载均衡**，公开模型称为 **模型映射**，别名称为
**路由别名**。

## Provider 模型

每个 Provider 维护一份它所服务的上游模型目录。一行包含：

| 字段 | 含义 |
| --- | --- |
| 上游模型 id | Provider 期望的 id。 |
| 显示名称 | 可选。 |
| 最大输入、最大输出 | 已知时的上下文窗口和输出上限。 |
| 支持思维 / 自适应 / 启用思维 | 能力标记，未设置表示未知。 |
| 变体 | 路由到该模型的额外名称，见下文。 |
| 启用 | 禁用的行不会被列出。 |

**拉取上游模型** 通过普通的列出模型路径、用你自己的密钥向 Provider 询问实时
目录并展示结果。在你勾选要导入的行之前不会写入任何内容；已有的行会被标记。
当内嵌默认目录认识某个模型时，会用其限制补齐空缺，并可为该 Provider 创建
默认价格规则。

**测试** 用你自己的密钥、经正常管线为该模型发送一次 16 Token 的 chat
completion。它经过准入、被计费，并报告状态码、延迟、付费的密钥，以及回复
或上游错误。

## 路由与成员

路由有名称、最大尝试次数和成员：

| 字段 | 含义 |
| --- | --- |
| Provider、上游模型 | 成员把流量发往何处。 |
| 固定凭证 | 可选，把成员限定在一个凭证上。 |
| 故障转移层级 | 默认 0。第 0 层全部不可用后，第 1 层才接收流量。 |
| 权重 | 默认 100，在同层健康成员之间分配流量。 |
| 启用 | 禁用的成员退出计划。 |

成员按层级、健康状态、权重排序。先用确定性的加权计数器在最低健康层级中选
出一个成员，再按 Provider 的策略在其中选出一个凭证。故障转移沿有序列表继续，
直到用尽路由的 **最大尝试次数**。失效凭证在消耗槽位之前就被排除；降级凭证
排在最后。

## 公开模型

**模型映射** 把一个公开名称绑定到一个路由。路由对外声明的能力由成员的
Provider 模型行保守折叠而来：

- 只有每个成员都声明了某项限制，该限制才算已知，并取最小值；
- 任一成员为 false 则能力标记为 `false`，全部为 true 才为 `true`，否则未知；
- 只有所有成员一致时才保留显示名称；
- 只有每个成员都声明了相同后缀，变体才会保留。

含 `/` 的公开名称构成一个 **命名空间**：`team-a/reviewer` 可以在
`/team-a/v1/...` 下以 `reviewer` 访问，`GET /team-a/v1/models` 只列出该
命名空间。

## 别名

别名把传入名称精确匹配到另一个名称。行按优先级排序，第一条启用且匹配的行
生效。

| 作用域 | 应用时机 |
| --- | --- |
| 任意 Provider | 路由查找之前，所有模式均适用。 |
| 单个 Provider | Provider 确定之后：发往该 Provider 的命名或 scoped 请求。 |

别名是精确字符串，不是模式。需要一族带后缀的名称时请使用变体。

## 变体与后缀预设

Provider 模型的 **变体** 字段声明路由到基础模型的额外名称。它保存为名称的
JSON 数组；当基础名称本身不应被列出时，则保存为对象：

```json
{ "expose_base": false, "variants": ["gpt-5-thinking-high", "gpt-5-tier-flex"] }
```

变体名称在整个目录中必须唯一。控制台的 **设置行为** 选择器按协议给出建议
后缀，并记录每个后缀注入的内容：

| 协议 | 后缀 | 请求字段 |
| --- | --- | --- |
| OpenAI Responses / Chat | `-thinking-none`、`-low`、`-medium`、`-high`、`-xhigh` | `reasoning.effort` / `reasoning_effort` |
| OpenAI Responses / Chat | `-tier-auto`、`-default`、`-flex`、`-scale`、`-priority`、`-fast` | `service_tier`（`-fast` = `priority`） |
| OpenAI Responses / Chat | `-effort-low`、`-medium`、`-high` | `text.verbosity` / `verbosity` |
| OpenAI Responses | `-image-generate`、`-image-edit`、`-search`、`-deep-research` | 强制 `tools` + `tool_choice` |
| Claude Messages | `-thinking-none`、`-low`、`-medium`、`-high`、`-adaptive` | `thinking`（预算 1024 / 10240 / 32768） |
| Claude Messages | `-effort-low`、`-medium`、`-high`、`-xhigh`、`-max` | `output_config.effort` |
| Gemini | `-thinking-none`、`-low`、`-medium`、`-high` | `generationConfig.thinkingConfig.thinkingLevel` |
| OpenRouter、Vercel | `-via-<source>` | `provider.only` / `providerOptions.gateway.only` |

思维与服务档位后缀由核心自行应用：当请求名称是已声明的变体，且剥去可识别
后缀后恰为基础名称时，请求体中的 `model` 会被改写，并按目标协议写入上表字
段。其他行为都保存为普通的 `rewrite` 规则，按变体名称过滤，放在控制台为每个
Provider 创建的规则集中（名为 `<provider> · defaults`）。可在
[路由规则与规则集](/zh-cn/guides/rules/)中查看和编辑。

## 模型列表

`GET /v1/models`（以及 Claude 和 Gemini 的列表路径）在聚合与命名空间模式下
由本地回答。列表是以下三者的并集：

1. 公开模型及其变体，带折叠后的元数据；
2. 以 `provider/model` 形式给出的各 Provider 目录；
3. 从计划中所有开启了 `auto_refresh_models`（默认开启）的 Provider 并发拉取
   的实时结果。

运营方的行优先于线上返回：你禁用的行绝不会出现，你记录的行保留你设置的限
制。刷新绝不写入目录。`GET /v1/models/{id}` 在同一列表中查找。两种操作都经
过准入，并记录一次零成本结算。像 `GET /openai-main/v1/models` 这样的命名请
求则遵循该 Provider 的路由规则。

权限在 Provider 和操作组层面过滤调用方可见、可调用的内容，见
[权限、限流与配额](/zh-cn/guides/permissions/)。
