---
title: 凭据选择与 Cache Affinity
description: 选择模式、内部 Cache Affinity 池设计、命中判定，以及 OpenAI/Claude/Gemini 缓存命中实践。
---

## 为什么需要这个页面

当提供商有多个凭据时，缓存命中率和凭据选择是耦合的：

- 如果缓存敏感的请求频繁切换凭据，上游缓存命中率通常会下降。
- 如果所有请求都固定到一个凭据，吞吐量和故障转移能力会降低。

GPROXY 通过选择模式加内部内存 Cache Affinity 池来平衡二者。

## 选择模式配置

在 `channels.settings` 中配置以下字段：

- `credential_round_robin_enabled`（默认 `true`）
- `credential_cache_affinity_enabled`（默认 `true`）
- `credential_cache_affinity_max_keys`（默认 `4096`）

生效模式：

| `credential_round_robin_enabled` | `credential_cache_affinity_enabled` | 生效模式 | 行为 |
|---|---|---|---|
| `false` | `false/true` | `StickyNoCache` | 无轮转，无 Affinity 池，始终选择最小可用 credential id |
| `true` | `true` | `RoundRobinWithCache` | 在符合条件的凭据间轮转，带 Affinity 匹配 |
| `true` | `false` | `RoundRobinNoCache` | 在符合条件的凭据间轮转，无 Affinity 匹配 |

说明：

- `StickyWithCache` 有意不支持。
- 禁用 Round-robin 时，Affinity 强制关闭。
- 旧字段 `credential_pick_mode` 仍可解析，保持兼容。

## 内部 Affinity 池设计

GPROXY 维护一个进程本地的映射：

- key: `"{channel}::{affinity_key}"`
- value: `{ credential_id, expires_at }`
- 存储: `DashMap<String, CacheAffinityRecord>`
- 每个 Channel 最多保留 `credential_cache_affinity_max_keys` 个 key；插入新 key 前，先清除已过期的 key，如果仍超限则淘汰最早过期的 key

该池为进程本地，不持久化，重启后重置。

## 命中判定与重试行为

`RoundRobinWithCache` 使用多候选提示：

- `CacheAffinityHint { candidates, bind }`
- 每个 candidate 包含 `{ key, ttl_ms, key_len }`

选择流程：

1. 根据请求体的协议特定块/前缀规则构建候选 key。
2. 按顺序扫描候选 key。遇到第一个未命中、或命中但对应凭据当前不可用时停止扫描。
3. 对连续命中的前缀，按凭据汇总 `key_len` 并选择总分最高的凭据。
4. 如果分数相同，选择在当前可用列表中排在前面的凭据。
5. 如果没有可用的候选，回退到普通 Round-robin。
6. 成功后，始终绑定 `bind` key 并刷新匹配的 key（如有）。
7. 如果 Affinity 选中的凭据请求失败并重试，仅清除该次尝试匹配的 key。

重要说明：

- 这是 GPROXY 内部的路由 Affinity，而非上游提供商的原生缓存命中判定。

## Key 派生与 TTL 规则（按协议）

对于内容生成请求，GPROXY 不再使用完整请求体哈希，而是使用规范化的块前缀。

通用规则：

- 每个块的规范 JSON：排序对象键，移除 `null`，数组保持顺序。
- 滚动前缀哈希：`prefix_i = sha256(seed + block_1 + ... + block_i)`。
- 非 Claude 候选采样：
  - `<=64` 个边界：全部包含
  - `>64`：取前 8 个和后 56 个
  - 匹配优先级：最长前缀优先
- `stream` 不参与 key 派生。

### OpenAI Chat Completions

块顺序：

- `tools[]`
- `response_format.json_schema`
- `messages[]`（按内容块拆分）

Key 格式：

- `openai.chat:ret={ret}:k={prompt_cache_key_hash|none}:h={prefix_hash}`

TTL：

- `prompt_cache_retention == "24h"` -> 24h
- 其他 -> 5m

### OpenAI Responses

块顺序：

- `tools[]`
- `prompt(id/version/variables)`
- `instructions`
- `input`（按 item/content 块拆分）

Key 格式：

- `openai.responses:ret={ret}:k={prompt_cache_key_hash|none}:h={prefix_hash}`

TTL：

- `prompt_cache_retention == "24h"` -> 24h
- 其他 -> 5m

不参与前缀 key 的字段：

- `reasoning`
- `max_output_tokens`
- `stream`

### Claude Messages

块层级：

- `tools[] -> system -> messages.content[]`
- Claude 简写在拆分前会被规范化：`system: "..."` 和 `messages[*].content: "..."` 各自转为一个 text 块

断点规则：

- 显式断点：块包含 `cache_control`
- 自动断点：存在顶层 `cache_control` 时，使用最后一个可缓存的块（必要时向前回退）

候选生成：

- 对每个断点，向前回溯最多 20 个边界
- 合并去重候选
- 优先级：靠后的断点优先，然后是更长的前缀优先

Key 格式：

- `claude.messages:ttl={5m|1h}:bp={explicit|auto}:h={prefix_hash}`

TTL：

- 显式或自动断点 `ttl == "1h"` -> 1h
- 显式或自动断点 `ttl == "5m"` -> 5m
- 存在 `cache_control` 但未指定 `ttl`（`{"type":"ephemeral"}`）时，内置 Claude 默认使用 5m

如果请求没有显式断点且没有顶层 `cache_control`，则不生成 Affinity 提示。

重要的顺序约束：

- Anthropic 会按处理层级（`tools -> system -> messages`）校验断点 TTL 顺序。
- `ttl="1h"` 的断点不能出现在 `ttl="5m"` 断点之后。
- 如果混用 1h 和 5m，应将所有 1h 断点放在层级中更靠前的位置。

### Gemini GenerateContent / StreamGenerateContent

如果存在 `cachedContent`：

- key: `gemini.cachedContent:{sha256(cachedContent)}`
- TTL: 60m

否则使用前缀模式：

- 块顺序：`systemInstruction -> tools[] -> toolConfig -> contents[].parts[]`
- key: `gemini.generateContent:prefix:{prefix_hash}`
- TTL: 5m

默认不参与 key 派生的字段：

- `generationConfig`
- `safetySettings`

## Claude / ClaudeCode 缓存改写与触发器

`enable_top_level_cache_control` 已弃用，请使用 `cache_breakpoints` 替代。

`claude` / `claudecode` 的改写来源：

- 提供商级别 `channels.settings.cache_breakpoints`
- 请求负载中已有的 `cache_control`（保持原样）
- `system[].text` 和 `messages[].content[].text` 中的魔法触发字符串

魔法触发行为：

- GPROXY 在转发上游前从文本中移除触发 token
- 如果目标块没有 `cache_control`，GPROXY 注入一个
- 如果块已有 `cache_control`，仅执行 token 移除
- 魔法触发注入的断点加上请求中已有的断点，总计上限为 4 个
- 断点预算耗尽后，GPROXY 仍移除触发 token，但跳过新的 `cache_control` 注入

支持的触发 token：

- `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH`
  - 注入 `{"type":"ephemeral"}`
- `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF`
  - 注入 `{"type":"ephemeral","ttl":"5m"}`
- `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY`
  - 注入 `{"type":"ephemeral","ttl":"1h"}`

## 上游缓存机制（提供商侧）

以下是提供商的行为，与 GPROXY 内部的 Affinity 机制无关。

### OpenAI

- Prompt 缓存基于精确前缀匹配。
- 请求按前缀哈希路由；`prompt_cache_key` 与路由 key 组合使用。
- 将静态内容（instructions/tools/examples/images schema）放在前缀中，可变内容放在尾部。
- 保留策略（`in_memory` vs `24h`）影响生命周期行为，但缓存匹配仍是前缀导向的。

### Claude

- 前缀层级为 `tools -> system -> messages`。
- 支持显式的块级断点和自动顶层 `cache_control`。
- 使用向后顺序检查，围绕断点有 20 块的回溯窗口。
- 可缓存性取决于块的资格；排序和断点位置直接影响命中率。

### Gemini

- 显式上下文缓存以 `cachedContent` 复用为核心（缓存内容被视为 prompt 前缀）。
- 隐式缓存由提供商管理，在短时间内发送相似前缀时效果最好。
- 复用相同的 `cachedContent` handle 通常能提高显式缓存命中率。
- GPROXY 目前支持生成路由，不暴露缓存内容管理路由。

## 实践建议

1. 保持前缀内容字节稳定（model、tools、system、长上下文顺序不变）。
2. 对缓存敏感的流量使用 `RoundRobinWithCache`。
3. 在短缓存窗口内避免不必要的凭据切换。
4. 将差异很大的 prompt 工作负载拆分到不同的 Channel/提供商。
5. 需要确定性 Claude/ClaudeCode 行为时，优先使用显式 `cache_breakpoints` TTL（`5m` / `1h`）。
6. Gemini 工作流中尽量复用显式的 `cachedContent`。

## 使用示例

Round-robin + Cache Affinity：

```toml
[channels.settings]
credential_round_robin_enabled = true
credential_cache_affinity_enabled = true
```

Round-robin 无 Affinity：

```toml
[channels.settings]
credential_round_robin_enabled = true
credential_cache_affinity_enabled = false
```

无 Round-robin（固定使用最小 id 的可用凭据）：

```toml
[channels.settings]
credential_round_robin_enabled = false
```
