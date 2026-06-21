# 通用请求转换规则 —— 设计问题(草稿,供调研)

> 状态:**草稿 / 未定稿**。brainstorming 进行中,用户去调研其它方案的做法。
> 日期:2026-06-19
> 背景:替代当前一堆专用规则 kind,做成一个通用、声明式、面向用户的转换规则。

## 1. 目标

当前 v2 的请求改写规则是一堆**专用 kind**:`system_text` / `cache_breakpoint` / `cache_magic` / `rewrite` / `sanitize` / `header`。每加一种新改写就要改后端(新 `RuleConfig` 变体 + apply 逻辑 + 前端 + i18n)。

**目标:做成一个通用、声明式的转换规则**,让"描述一种请求改写"变成**写配置**,以后加功能基本**不动后端**。后端退化为一个**薄而听话的引擎**。

## 2. 约束 / 设计哲学(已对齐)

- **面向用户的配置面**:管理员写规则。**表达力 > 后端强制的安全**。
- **引擎"听话"不"管教"**:用户描述什么就做什么。哪怕结果会被上游拒(如 Claude 拒绝 >4 个缓存断点、拒绝在 thinking 块上打 cache_control),也照做,后果用户自负。**后端不加保姆式 guard**。
- **加新改写类型 ≈ 零后端改动**。
- 作用对象:**provider-native 请求**(JSON body + HTTP headers),在协议 transform **之后**、发上游**之前**。
- **协议/语义差异**(claude `system` vs openai `messages` vs gemini `systemInstruction`)由**前端预设**生成对应 path 来吸收;**后端不懂语义**。

## 3. 已收敛的模型:`locate + actions (+ limit)`

一条规则 = 定位 + 动作(+ 可选上限):

```jsonc
{
  "locate":  { "path": "system[-1]" }      // 结构定位:JSON 路径,支持负索引(-1=末)、* 通配
           | { "match": "<regex>" }        // 文本定位:正则扫文本字段;绑定"命中文字片段"和"包住它的 JSON 块"
           | { "header": "anthropic-beta" },// HTTP 头
  "limit":   4,                            // 可选:命中上界(主要对 match / 通配有意义)
  "actions": [ /* 按序执行,可多步 */
    { "op": "set",          "value": <json> },
    { "op": "merge",        "value": <json> },   // 深合并(cache_control 盖到块上靠它)
    { "op": "delete" },
    { "op": "insert",       "value": <json>, "at": "prepend" | "append" },
    { "op": "replace_text", "with": "<text>" }   // 替换正则命中的那段文字
  ]
}
```

**两个正交轴**:
- **WHERE(定位)= 正则(文本)或 路径(结构)二选一** —— 因为 JSON 结构不是正则语言,正则对序列化 JSON 做结构定位既脆又错;而路径找不了任意文本。
- **WHAT(动作)= 那几个 op**。

## 4. 现有 6 个功能如何用它描述

| 功能 | locate | actions / limit |
|---|---|---|
| sanitize | `match: 正则` | `replace_text` |
| cache_magic(魔法串) | `match: 魔法串正则` | `limit:4` + `replace_text:""`(剥串)→ `merge:{cache_control:{…}}` |
| cache_breakpoint(位置) | `path: system[-1]` | `merge:{cache_control:{…}}` |
| rewrite | `path: temperature` | `set` / `delete` / `merge` |
| system_text | `path: system` | `insert: {…}, at: prepend`(各协议 path 由前端预设给) |
| header | `header: anthropic-beta` | `set` / `merge` |

## 5. 关系决策(已定)

**B 方案**:后端只有这**一个通用 `transform` 规则**;前端提供「系统文本 / 缓存断点 / 魔法串 / 改字段 …」**预设/向导**,它们只是**生成**通用规则的 config。加新功能优先做成**前端预设**(纯前端);引擎没覆盖的事直接手写 locate+actions。现有 6 kind 迁移成预设。

## 6. 开放问题(调研重点)

1. **条件 `where` 要不要单列?** 倾向**不要**:文本类条件写进正则(lookaround/更精确模式),结构类靠 path 指准 + "听话"哲学,只留一个 `limit` 计数。—— 看别人是否有更好的轻量条件表达。
2. **`limit` 语义**:仅"本规则命中上界",还是要像 v1 那样**全局预算**(把已存在的 cache_control 也算进 4 上限)?倾向前者(听话、用户自负)。
3. **"匹配后改结构"(cache_magic)**:正则命中文本,动作改"包住它的块"。**"块"的边界**怎么干净定义。
4. **路径语法**:采用现成标准(JSON Pointer / JSONPath / JMESPath),还是自造小语法(点号/方括号 + 负索引 + `*`)?标准的负索引/通配支持各不相同。
5. **动作语义**:复用 JSON Patch(RFC 6902)/ JSON Merge Patch(RFC 7386),还是自造 set/merge/delete?
6. **headers** 纳入同一套规则,还是单独?

## 7. 可对照的现成方案(调研清单)

- **JSON Patch(RFC 6902)**:基于 JSON Pointer 的 add/remove/replace/move/copy/test。
- **JSON Merge Patch(RFC 7386)**:深合并语义。
- **JSONPath / JMESPath / JSONata**:查询+转换语言(JSONPath 有 `*`、过滤 `?()`;负索引/written-back 支持各异)。
- **jq**:转换语言。
- **JSONLogic**:把"条件"写成 JSON(若要 `where` 可参考)。
- **OPA / Rego**:策略 + 转换。
- **API 网关的请求改写**:Kong(request-transformer(-advanced))、Envoy(Lua/Wasm filter、header mutation)、Apigee(AssignMessage)、AWS API Gateway mapping template(VTL)。
- **LLM 网关**:LiteLLM、Portkey、Cloudflare AI Gateway、Helicone —— 看它们怎么表达请求改写 / guardrail。

> 三种反复出现的范式:(a) **指针+补丁**(JSON Patch)管结构;(b) **查询/转换语言**(JMESPath/JSONata)做 select+transform;(c) **条件语言**(JSONLogic/Rego)做判定。我们的 `locate(path|regex)+actions+limit` 介于 (a)(b) 之间,并把正则补进文本定位。

## 8. 待用户研究回来后定的事

- locate/actions 是自造还是贴标准(影响实现量与用户熟悉度)。
- `where` 去留、`limit` 语义。
- 现有规则数据迁移策略(v2 现有 rules 行 → 通用 transform config)。
- 然后:写正式 spec → 实现计划。

## English

# Generic Request Transform Rules - Design Questions (Draft)

> Status: **draft / not finalized**. Brainstorming is still in progress; the user
> is researching how other systems solve this.
> Date: 2026-06-19
> Background: replace the current set of specialized rule kinds with one generic,
> declarative, user-facing transform rule.

## 1. Goal

The current v2 request rewrite surface is a set of **specialized kinds**:
`system_text` / `cache_breakpoint` / `cache_magic` / `rewrite` / `sanitize` /
`header`. Each new rewrite type requires backend changes: a new `RuleConfig`
variant, apply logic, frontend, and i18n.

The goal is one generic declarative transform rule. Describing a request rewrite
should become **configuration**, so new features usually do **not** need backend
changes. The backend becomes a thin, obedient engine.

## 2. Constraints / Design Philosophy

- **User-facing configuration surface**: administrators write rules.
  **Expressiveness > backend-enforced safety**.
- The engine should be **obedient**, not paternalistic. If the user describes a
  mutation, do it, even if the upstream later rejects it, such as too many Claude
  cache breakpoints or cache control on a thinking block. The user owns the
  result.
- Adding a new rewrite type should be close to zero backend work.
- Scope: **provider-native request** JSON body plus HTTP headers, after protocol
  transform and before upstream send.
- Protocol/semantic differences, such as Claude `system` vs OpenAI `messages`
  vs Gemini `systemInstruction`, should be absorbed by **frontend presets** that
  generate the right paths. The backend should not understand those semantics.

## 3. Current Converged Model: `locate + actions (+ limit)`

One rule = locator + actions + optional limit:

```jsonc
{
  "locate":  { "path": "system[-1]" }       // structural JSON path, supports negative index and wildcard
           | { "match": "<regex>" }         // text locator over text fields; binds matched text and containing JSON block
           | { "header": "anthropic-beta" },// HTTP header
  "limit":   4,                             // optional hit cap, mainly for match/wildcard
  "actions": [
    { "op": "set",          "value": <json> },
    { "op": "merge",        "value": <json> },
    { "op": "delete" },
    { "op": "insert",       "value": <json>, "at": "prepend" | "append" },
    { "op": "replace_text", "with": "<text>" }
  ]
}
```

Two orthogonal axes:

- **WHERE / locator**: regex text match or structural path, but not both. JSON
  structure is not a regular language, so regex over serialized JSON is brittle
  for structural targeting; paths cannot locate arbitrary text.
- **WHAT / action**: a small set of operations.

## 4. Mapping Existing Six Features

| Feature | locate | actions / limit |
| --- | --- | --- |
| sanitize | `match: regex` | `replace_text` |
| cache_magic | `match: magic regex` | `limit:4` + `replace_text:""` then `merge:{cache_control:{...}}` |
| cache_breakpoint | `path: system[-1]` | `merge:{cache_control:{...}}` |
| rewrite | `path: temperature` | `set` / `delete` / `merge` |
| system_text | `path: system` | `insert: {...}, at: prepend`; protocol-specific path comes from frontend presets |
| header | `header: anthropic-beta` | `set` / `merge` |

## 5. Relationship Decision

Use option B: the backend has only this one generic `transform` rule. The
frontend provides presets/wizards such as "system text", "cache breakpoint",
"magic string", and "field rewrite"; these only generate generic rule config.
New features should first be implemented as **frontend presets**. If the engine
already covers the behavior, advanced users can write `locate + actions`
directly. The existing six kinds migrate into presets.

## 6. Open Questions

1. Should conditional `where` be a separate concept? Current leaning: **no**.
   Text conditions can live in regex lookarounds or precise patterns; structural
   conditions can use an exact path plus the obedient-engine philosophy. Keep
   only `limit`.
2. What is `limit` semantics? Only per-rule hit cap, or v1-style global budget
   that counts existing `cache_control` too? Current leaning: per-rule hit cap.
3. How to define "match text, then modify containing structure" for
   `cache_magic`? The containing block boundary still needs a clean definition.
4. Path syntax: use an existing standard such as JSON Pointer, JSONPath, or
   JMESPath, or define a small dot/bracket syntax with negative index and `*`?
5. Action semantics: reuse JSON Patch / JSON Merge Patch, or keep custom
   `set` / `merge` / `delete`?
6. Should headers be part of the same rule system or remain separate?

## 7. Existing Systems To Compare

- **JSON Patch (RFC 6902)**: JSON Pointer plus add/remove/replace/move/copy/test.
- **JSON Merge Patch (RFC 7386)**: merge semantics.
- **JSONPath / JMESPath / JSONata**: query and transform languages.
- **jq**: transformation language.
- **JSONLogic**: JSON-shaped condition language, useful if `where` returns.
- **OPA / Rego**: policy plus transformation.
- **API gateways**: Kong request-transformer, Envoy Lua/Wasm filters, Apigee
  AssignMessage, AWS API Gateway VTL mappings.
- **LLM gateways**: LiteLLM, Portkey, Cloudflare AI Gateway, Helicone.

Three recurring patterns:

- pointer plus patch for structure;
- query/transform language for select plus transform;
- condition language for decisions.

The current `locate(path|regex) + actions + limit` model sits between pointer
patching and query/transform languages, with regex added for text targeting.

## 8. Decisions After Research

After the external research comes back, decide:

- whether `locate/actions` should be custom or standards-based;
- whether `where` exists and what `limit` means;
- how to migrate existing v2 `rules` rows into generic transform config;
- then write the formal spec and implementation plan.
