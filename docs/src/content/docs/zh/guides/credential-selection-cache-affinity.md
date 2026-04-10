---
title: 凭证选择与缓存亲和池
description: 凭证选择模式、内部缓存亲和池设计、命中判定，以及 OpenAI/Claude/Gemini 的缓存命中实践。
---

## 为什么需要这一页

一个 provider 配置多凭证时，缓存命中率与凭证选择策略强耦合：

- 缓存敏感请求频繁换凭证，通常会降低上游缓存命中率。
- 所有流量固定单凭证，会降低吞吐与故障切换能力。

GPROXY 用“凭证选择模式 + 进程内缓存亲和池”来平衡这两点。

## 凭证选择模式

在 `channels.settings` 中配置：

- `credential_round_robin_enabled`（默认 `true`）
- `credential_cache_affinity_enabled`（默认 `true`）
- `credential_cache_affinity_max_keys`（默认 `4096`）

最终模式：

| `credential_round_robin_enabled` | `credential_cache_affinity_enabled` | 最终模式 | 行为 |
|---|---|---|---|
| `false` | `false/true` | `StickyNoCache` | 不轮询，不用亲和池，始终选当前可用凭证里 id 最小者 |
| `true` | `true` | `RoundRobinWithCache` | 在可用凭证中轮询并启用亲和匹配 |
| `true` | `false` | `RoundRobinNoCache` | 在可用凭证中轮询，不做亲和匹配 |

说明：

- `StickyWithCache` 已移除，不再支持。
- 关闭轮询时会强制关闭亲和池。
- 历史字段 `credential_pick_mode` 仍会被解析。

## 内部缓存亲和池设计（v1）

GPROXY 维护进程内 map：

- key：`"{channel}::{affinity_key}"`
- value：`{ credential_id, expires_at }`
- 存储：`DashMap<String, CacheAffinityRecord>`
- 每个渠道最多保留 `credential_cache_affinity_max_keys` 个键；插入新键前会先清理过期键，仍超限时淘汰最早过期的键

这仍是 v1 设计：不引入 v2 key 前缀，不改存储结构。

## 命中判定与重试行为

仅 `RoundRobinWithCache` 使用多候选 hint：

- `CacheAffinityHint { candidates, bind }`
- 每个候选为 `{ key, ttl_ms, key_len }`

处理流程：

1. 按协议分块规则构建候选键（有优先级顺序）。
2. 按顺序扫描候选键；遇到首个未命中，或命中到当前不可用凭证时立即停止继续扫描。
3. 对“连续命中的前缀”按凭证累计 `key_len`，选择总分最高的凭证。
4. 若总分并列，选择当前可用列表里位置更靠前的凭证。
5. 若没有可用候选，退回普通轮询。
6. 请求成功后，总是写入 `bind` 键，并在有命中键时刷新命中键 TTL。
7. 若本次亲和命中失败并重试，只清理本次命中的那个键。

注意：

- 以上是 GPROXY 内部“路由亲和”逻辑，不等同于上游厂商原生缓存命中判定。

## 协议键推导与 TTL 规则

四类内容生成请求不再使用整包 body hash，而是按“可缓存前缀分块 + 滚动哈希”。

统一规则：

- 块级 canonical JSON：对象 key 排序、去除 `null`、数组保序。
- 滚动哈希：`prefix_i = sha256(seed + block_1 + ... + block_i)`。
- 非 Claude 采样：
  - 边界 `<=64` 全量
  - 边界 `>64` 取“前8 + 后56”
  - 优先级按最长前缀优先
- `stream` 不参与键计算。

### OpenAI Chat Completions

分块顺序：

- `tools[]`
- `response_format.json_schema`
- `messages[]`（按 content block 细分）

键格式：

- `openai.chat:ret={ret}:k={prompt_cache_key_hash|none}:h={prefix_hash}`

TTL：

- `prompt_cache_retention == "24h"` -> 24h
- 其他 -> 5m

### OpenAI Responses

分块顺序：

- `tools[]`
- `prompt(id/version/variables)`
- `instructions`
- `input`（按 item/content block 细分）

键格式：

- `openai.responses:ret={ret}:k={prompt_cache_key_hash|none}:h={prefix_hash}`

TTL：

- `prompt_cache_retention == "24h"` -> 24h
- 其他 -> 5m

默认不参与前缀键：

- `reasoning`
- `max_output_tokens`
- `stream`

### Claude Messages

分块层级：

- `tools[] -> system -> messages.content[]`
- Claude shorthand 会先规范化再拆分：`system: "..."` 和 `messages[*].content: "..."` 都视为一个 text block

断点来源：

- 显式断点：块上有 `cache_control`
- 顶层自动断点：请求有顶层 `cache_control` 时，取最后可缓存块（必要时向前回退）

候选构建：

- 每个断点最多回看 20 个边界
- 合并去重
- 优先级：更晚断点优先；同断点内更长前缀优先

键格式：

- `claude.messages:ttl={5m|1h}:bp={explicit|auto}:h={prefix_hash}`

TTL：

- 显式或自动断点 `ttl == "1h"` -> 1h
- 显式或自动断点 `ttl == "5m"` -> 5m
- 当存在 `cache_control` 但未写 `ttl`（`{"type":"ephemeral"}`）时，built-in Claude 默认按 5m 处理

若既无显式断点也无顶层 `cache_control`，则不生成 Claude 亲和 hint。

顺序约束（重要）：

- Anthropic 会按 `tools -> system -> messages` 校验 TTL 顺序。
- `ttl="1h"` 不能出现在 `ttl="5m"` 之后。
- 混用 `1h` 与 `5m` 时，请把所有 `1h` 断点放在更靠前层级。

### Gemini GenerateContent / StreamGenerateContent

若存在 `cachedContent`：

- 键：`gemini.cachedContent:{sha256(cachedContent)}`
- TTL：60m

否则走前缀模式：

- 分块顺序：`systemInstruction -> tools[] -> toolConfig -> contents[].parts[]`
- 键：`gemini.generateContent:prefix:{prefix_hash}`
- TTL：5m

默认不纳入键：

- `generationConfig`
- `safetySettings`

## Claude / ClaudeCode 缓存改写与 Magic Trigger

当前配置文档中，`enable_top_level_cache_control` 已不再作为推荐方案，统一改为 `cache_breakpoints`。

`claude` / `claudecode` 的改写来源：

- provider 侧 `channels.settings.cache_breakpoints`
- 请求体已有的 `cache_control`（原样保留）
- `system[].text` 与 `messages[].content[].text` 里的 magic trigger 字符串

Magic trigger 行为：

- gproxy 在上游转发前会先删除触发串
- 若目标块还没有 `cache_control`，则注入对应 `cache_control`
- 若块上已存在 `cache_control`，则只做字符串删除，不覆盖原配置
- magic trigger 新增断点与请求里已有断点合计最多 4 条
- 当 4 条预算耗尽时，gproxy 仍会删除触发串，但不再新增 `cache_control`

支持的触发串：

- `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH auto`
  - 注入 `{"type":"ephemeral"}`
- `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF 5m`
  - 注入 `{"type":"ephemeral","ttl":"5m"}`
- `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY 1h`
  - 注入 `{"type":"ephemeral","ttl":"1h"}`

## 上游缓存机制（与 GPROXY 内部实现解耦）

### OpenAI

- 基于“精确前缀匹配”。
- 请求会按前缀哈希路由；`prompt_cache_key` 会与路由键组合。
- 建议把 instructions/tools/examples/images schema 放在前缀，把可变内容放在尾部。
- retention（`in_memory` / `24h`）影响保留时长，但命中本质仍是前缀匹配。

### Claude

- 前缀层级是 `tools -> system -> messages`。
- 支持显式 block 断点与顶层 `cache_control` 自动缓存。
- 断点命中使用“倒序顺扫 + 20 block lookback”。
- 对块顺序、断点位置、可缓存块边界高度敏感。

### Gemini

- 显式缓存主要依赖 `cachedContent` 复用（缓存内容作为 prompt 前缀）。
- 隐式缓存由上游自动管理，短时间内发送相似前缀更易命中。
- 复用同一 `cachedContent` 句柄通常更容易获得显式缓存收益。
- 当前 GPROXY 仅覆盖内容生成，不提供 `cachedContent` 管理路由。

## 提高命中率建议

1. 保持前缀稳定：model、tools、system、长上下文顺序尽量不变。
2. 缓存敏感业务用 `RoundRobinWithCache`。
3. 短缓存窗口内避免凭证频繁抖动。
4. Prompt 差异很大的流量拆分到不同 provider/channel。
5. 需要可预测行为时，优先在 `cache_breakpoints` 里显式写 TTL（`5m` / `1h`）。
6. Gemini 场景尽量复用 `cachedContent`。

## 配置示例

轮询 + 亲和池：

```toml
[channels.settings]
credential_round_robin_enabled = true
credential_cache_affinity_enabled = true
```

轮询 + 不使用亲和池：

```toml
[channels.settings]
credential_round_robin_enabled = true
credential_cache_affinity_enabled = false
```

不轮询（固定最小 id 可用凭证）：

```toml
[channels.settings]
credential_round_robin_enabled = false
```
