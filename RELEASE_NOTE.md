# Release Notes

## v1.0.14

> Console rewrite-rule pipeline is repaired end-to-end: the `Set` / `Remove` action tags are now emitted in the snake_case form the backend actually accepts, manually drafted rules no longer disappear on Save (stale-closure race), suffix variants auto-attach a `model`-rename rewrite so the upstream receives the real model id instead of the alias, and the Claude thinking presets now explicitly set `display: "summarized"` so the chain-of-thought stays visible in responses. Claude Opus 4.7 pricing is also shipped in the built-in `anthropic.json` table.

### English

#### Added

- **Claude Opus 4.7 pricing in the built-in anthropic pricing table.** `sdk/gproxy-channel/src/channels/pricing/anthropic.json` now contains a `claude-opus-4-7` entry with both default-tier and priority-tier pricing (input $5 / output $25 / cache-read $0.5 / 5m-write $6.25 / 1h-write $10 per 1M default; priority 6× default). New Opus 4.7 providers get accurate billing out of the box — no manual `Apply Default Pricing` needed.
- **Suffix-variant rewrite now auto-renames `body.model` to the real model id.** `addSuffixVariant` appends a final `{ path: "model", action: { type: "set", value: <real_model_id> } }` rule after the parameter-setting rules (thinking / reasoning / effort / tier / verbosity). Without this, the outbound request still carried the alias string (e.g. `claude-opus-4-7-thinking-high`) in `body.model` and upstream rejected it — alias resolution only rewrote routing metadata, not the body. Ordering matters: the rename is last so the other rules can still match against the alias via `model_pattern`.
- **Claude thinking presets set `display: "summarized"` explicitly.** `-thinking-low` / `-thinking-medium` / `-thinking-high` / `-thinking-adaptive` in `suffix-presets.ts` now pin `display` so the chain-of-thought stays visible in responses regardless of future default-behavior changes. `-thinking-none` (disabled) intentionally has no `display` field (Claude's disabled variant doesn't accept one).

#### Fixed

- **Console emits snake_case rewrite-action tags.** The backend `RewriteAction` enum uses `#[serde(rename_all = "snake_case")]` and rejected the capitalized `"Set"` / `"Remove"` tags the console had been writing, producing `unknown variant 'Set', expected 'set' or 'remove'` on every save. The TypeScript `RewriteAction` type and every writer in `ModelsPane` / `RewriteRuleEditor` / `RewriteRulesTab` / `channel-constants` now use the lowercase form. `normalizeRewriteAction` still accepts the legacy capitalized tags on read so already-persisted configs render correctly.
- **Manually drafted rewrite rule no longer vanishes after Save.** `RewriteRulesTab.save()` committed the new draft via `setProviderForm`, then immediately called the parent's `saveProvider`, which captured `providerForm` from its render-time closure — the queued state update had not flushed yet, so the POST body omitted the new rule, and the following `reloadAndReselect` overwrote local state with the (unchanged) backend version. `onSave` now accepts an optional `rewriteRulesOverride: string`, and the draft-commit path hands the freshly-computed JSON to the parent so `saveProvider` substitutes it into the payload instead of reading the stale closure.

#### Changed

- **"Channel" form label → "Channel Type" (both locales).** The dropdown selects one of ~12 built-in channel kinds (anthropic, claudecode, codex, ...), not a channel instance. The old label read as if it were picking an instance.
- **Transform match statements simplified with guard clauses.** Pure readability refactor across 8 response-transform files (Claude → OpenAI / Gemini, Gemini → Claude / OpenAI Response, OpenAI Chat → Claude, OpenAI Response → Claude). No behavior change.

#### Compatibility

- **Drop-in upgrade** from v1.0.13. No DB migration, no HTTP API change, no config change at the surface level.
- **Console rewrite-rule snake_case migration is read-compatible.** Any rewrite rules saved with the old capitalized tags still render and match; the next save rewrites them as snake_case. No manual cleanup required.
- **SDK / protocol consumers**: no protocol surface changes in this release.

### 简体中文

#### 新增

- **内置 anthropic 价目表新增 Claude Opus 4.7 定价。** `sdk/gproxy-channel/src/channels/pricing/anthropic.json` 新增 `claude-opus-4-7` 条目,同时包含默认档和 priority 档单价(默认 1M tokens:input $5 / output $25 / cache-read $0.5 / 5m-write $6.25 / 1h-write $10;priority 档 6×)。新建 Opus 4.7 provider 可以直接用内置模板计费,不用手点 `Apply Default Pricing`。
- **后缀变体的 rewrite 规则现在自动把 `body.model` 改写回真实模型名。** `addSuffixVariant` 会在参数规则(thinking / reasoning / effort / tier / verbosity)之后再追加一条 `{ path: "model", action: { type: "set", value: <真实模型名> } }`。之前请求体里的 `body.model` 仍然是别名(比如 `claude-opus-4-7-thinking-high`),上游不识别 —— 别名解析只改了路由元数据,没碰 body。改写必须放在最后,否则前面基于 `model_pattern` 匹配别名的规则会被自己写坏而失配。
- **Claude thinking 预设显式写入 `display: "summarized"`。** `suffix-presets.ts` 里 Claude 的 `-thinking-low` / `-thinking-medium` / `-thinking-high` / `-thinking-adaptive` 四档现在固定 `display: "summarized"`,确保响应里的思维链始终可见,不依赖 API 默认值将来是否变化。`-thinking-none`(disabled) 故意不带 `display`(Claude disabled 分支不接受这个字段)。

#### 修复

- **控制台写出 snake_case 的 rewrite action tag。** 后端 `RewriteAction` 使用 `#[serde(rename_all = "snake_case")]`,此前 console 写的 `"Set"` / `"Remove"` 会直接被拒,保存时报 `unknown variant 'Set', expected 'set' or 'remove'`。TypeScript 里的 `RewriteAction` 类型和 `ModelsPane` / `RewriteRuleEditor` / `RewriteRulesTab` / `channel-constants` 所有写入点统一改为小写;`normalizeRewriteAction` 在读取路径保留了对历史大写值的兼容,旧配置仍能正常展示。
- **手动新增的 rewrite rule 保存后不再消失。** `RewriteRulesTab.save()` 在草稿提交时先调 `setProviderForm` 写入新规则,然后立刻调用父组件的 `saveProvider` —— 但 `saveProvider` 闭包里的 `providerForm` 是上一次渲染时的值,队列里的 state update 还没刷到闭包,POST 发出的是不含新规则的旧 JSON;接着 `reloadAndReselect` 又用后端(没保存上的)旧值覆盖本地,新规则就这样蒸发了。`onSave` 新增可选参数 `rewriteRulesOverride: string`,草稿提交分支把刚算出的 JSON 直接传给父组件,`saveProvider` 用它替换 payload 里的 `rewrite_rules`,不再依赖陈旧闭包。

#### 调整

- **表单 "Channel" 标签 → "渠道类型" / "Channel Type"(两种语言均改)。** 这个下拉选的是 ~12 种内置渠道类型(anthropic / claudecode / codex / ...),不是具体的渠道实例,旧标签读着像在选实例。
- **Transform 中 match 语句用 guard clause 简化。** 纯可读性重构,覆盖 8 个响应转换文件(Claude → OpenAI / Gemini、Gemini → Claude / OpenAI Response、OpenAI Chat → Claude、OpenAI Response → Claude),行为不变。

#### 兼容性

- **从 v1.0.13 直接升级**。无 DB 迁移、无 HTTP API 变更、无表面配置变更。
- **Rewrite 规则 snake_case 迁移对读向后兼容。** 历史大写 tag 保存的规则仍能正常渲染和匹配;下次保存会以 snake_case 写回。无需手动清理。
- **SDK / protocol 调用方**:本版本无协议表面变化。

## v1.0.13

> `gproxy-protocol` is updated for Claude Opus 4.7: the Claude wire types now include the new model / output fields (`claude-opus-4-7`, `output_config.task_budget`, `effort="xhigh"`), and Claude-targeting transforms stop generating deprecated budgeted `thinking: { type: "enabled" }` requests when the target model is Opus 4.7.

### English

#### Added

- **Claude Opus 4.7 protocol fields in `gproxy-protocol`.** Claude request types now recognize `claude-opus-4-7`, support `output_config.task_budget`, and accept the new `output_config.effort = "xhigh"` value. This keeps the L0 wire types aligned with the current Claude Messages API surface.
- **Regression tests for Opus 4.7 request shaping.** Added unit coverage for `claude-opus-4-7` model serialization, `xhigh` + `task_budget` output config serialization, and the Opus-4.7-specific thinking conversion paths in Gemini → Claude and OpenAI → Claude transforms.

#### Fixed

- **Claude-targeting transforms no longer emit removed extended-thinking budgets for Opus 4.7.** When the target Claude model is `claude-opus-4-7`, the OpenAI → Claude and Gemini → Claude request transforms now map reasoning / thinking to adaptive thinking instead of constructing `thinking: { type: "enabled", budget_tokens: ... }`, which Claude Opus 4.7 rejects.
- **Claude output-effort mappings now understand `xhigh`.** Claude → OpenAI and Claude → Gemini transforms now treat `BetaOutputEffort::XHigh` as a first-class value instead of only handling `low` / `medium` / `high` / `max`, keeping verbosity / reasoning-effort conversions internally consistent.

#### Compatibility

- **Drop-in upgrade** from v1.0.12 for the gproxy server and console. No DB migration, no HTTP API route change, no config change, and no non-protocol crate behavior change.
- **SDK / protocol consumers**: additive protocol update. If you construct Claude payloads through `gproxy-protocol`, you can now use the Opus 4.7 model id and the new output config fields directly. Existing payloads continue to deserialize as before.

### 简体中文

#### 新增

- **`gproxy-protocol` 补齐 Claude Opus 4.7 协议字段。** Claude 请求类型现在识别 `claude-opus-4-7`,支持 `output_config.task_budget`,并接受新的 `output_config.effort = "xhigh"` 值,让 L0 wire types 与当前 Claude Messages API 对齐。
- **新增 Opus 4.7 请求 shape 回归测试。** 增加了 `claude-opus-4-7` 模型序列化、`xhigh` + `task_budget` 输出配置序列化,以及 Gemini → Claude / OpenAI → Claude 在 Opus 4.7 场景下 thinking 转换路径的单测。

#### 修复

- **指向 Claude 的 transform 不再为 Opus 4.7 生成已移除的 extended-thinking budget 形状。** 当目标模型是 `claude-opus-4-7` 时,OpenAI → Claude 与 Gemini → Claude 的请求转换现在会把 reasoning / thinking 映射为 adaptive thinking,不再构造 `thinking: { type: "enabled", budget_tokens: ... }` 这种会被 Claude Opus 4.7 拒绝的请求。
- **Claude output-effort 映射补齐 `xhigh`.** Claude → OpenAI 和 Claude → Gemini 的 transform 现在把 `BetaOutputEffort::XHigh` 作为一等值处理,不再只覆盖 `low` / `medium` / `high` / `max`,避免 verbosity / reasoning-effort 转换前后不一致。

#### 兼容性

- **从 v1.0.12 直接升级**。对 gproxy server 和 console 来说,不涉及 DB 迁移、HTTP API 路由变化、配置变化,也没有非 protocol crate 的行为改动。
- **SDK / protocol 调用方**:这是一次增量协议更新。如果你直接用 `gproxy-protocol` 构造 Claude payload,现在可以直接使用 Opus 4.7 的 model id 和新的 output config 字段。现有 payload 的反序列化行为保持不变。

## v1.0.12

> Proxy response headers are now normalized (correct `Content-Type`, stripped upstream `Content-Length` / `Content-Encoding` / `Transfer-Encoding`), two long-standing bugs in the OpenAI-response → Claude stream converter are fixed (duplicate block emission when `output_item.done` arrives after streamed deltas; spurious `stop_reason=end_turn` swallowing `tool_use`), the OpenAI WebSocket handshake now detects auth failures on the first frame and rotates to the next credential, and the "dispatch" concept is renamed to "routing" across SDK / API / storage / console / docs — with an automatic SQL column rename from `providers.dispatch_json` to `providers.routing_json`.

### English

#### Added

- **`Apply Default Pricing` button on the Models tab.** The 12 backend per-channel pricing JSON files (397 models total) are consolidated into a frontend lookup table at `frontend/console/src/lib/default-model-pricing.ts`. Each model's edit form now exposes a button that auto-fills `pricing_json` by taking the last `/`-separated segment of `model_id` and running a longest-substring match against the template — one click to populate pricing for any model the template knows.
- **OpenAI Responses WebSocket auth probe.** `UpstreamWebSocket` now waits up to 150ms for the first upstream frame when operation is `OpenAiResponseWebSocket`; if it classifies as a 401/403 / `invalid_api_key` / permission / unauthorized signal, the credential is marked dead and the engine rotates to the next one. Non-auth first frames are buffered and delivered on the first `recv()` so downstream code sees no dropped data. Before this, a bad `sk-proj-…` key produced a successful `101 Switching Protocols`, an immediate error frame, and a user-facing failure with no credential rotation.
- **`prepare_ws_auth` returns credential indices with round-robin ordering.** The WS auth candidate tuple is now `(credential_index, url, headers)` instead of `(url, headers)`. The runtime filters dead credentials up-front (cooldown-health aware) and rotates the start offset via an atomic cursor, matching HTTP execution semantics.
- **`parseBetaHeaders` accepts JSON array strings.** Legacy CSV input (`"a,b,c"`) is replaced by strict JSON array parsing (`'["a","b","c"]'`) so the `BetaHeadersEditor` can round-trip structured config without ambiguity. Invalid input yields `[]` instead of silent partial parse. Covered by new unit tests.

#### Fixed

- **Proxy response headers now normalized.** The new `normalize_response_headers` helper strips three upstream-owned headers (`Content-Length`, `Content-Encoding`, `Transfer-Encoding`) from every `proxy`, `proxy_unscoped`, and `proxy_unscoped_files` response because the body is re-streamed through axum and the stale values break chunked encoding / gzip-chained downstreams. When the upstream omitted `Content-Type` entirely, a correct default is injected per (operation, protocol) — `text/event-stream` for Claude / OpenAI-chat / OpenAI-response / Gemini streaming, `application/json` for non-stream generation / count-token / compact / embedding / image / file / model-list routes.
- **`OpenAiResponseToClaudeStream` no longer double-emits closed blocks.** The converter kept per-block sets (`completed_text_blocks` / `completed_thinking_blocks` / `completed_summary_blocks` / `streamed_message_items` / `streamed_tool_args`) so a `*.done` event that arrives after the corresponding streaming delta closes the already-open block exactly once, and tool-call `output_item.done` with the same `item_id` as a streamed `function_call_arguments.done` becomes a single `content_block_stop` instead of a re-opened block. The rewrite consolidates the duplicate per-event block-close logic into `finish_text_block` / `finish_thinking_block` / `finish_summary_block` helpers.
- **`OpenAiResponseToClaudeStream` preserves `tool_use` stop reason.** On a `ResponseStreamEvent::Completed` with no `incomplete_details.reason`, the converter previously forced `stop_reason = BetaStopReason::EndTurn`, which overwrote the `ToolUse` reason set by the tool-call mapper. It now leaves `stop_reason` as `None` in that branch so tool-driven stop reasons propagate to the final `message_delta`. Regression-tested with a function-call → completed sequence that asserts `BetaStopReason::ToolUse`.
- **Pricing save: missing `model_id` and i64 overflow.** `ModelPrice.model_id` gains `#[serde(default)]` because the frontend omits it (backend overwrites from the URL param) and the previous hard requirement caused 400 on save. Pricing templates' "unlimited" tier cap was lowered from `i64::MAX` (`9_223_372_036_854_775_807`) to `100_000_000` — JavaScript rounds `i64::MAX` to `9_223_372_036_854_776_000` on `JSON.parse`, which overflows i64 on round-trip. 100M tokens is still effectively unlimited (no LLM has a context window anywhere near it).
- **Dashboard i18n.** `dashboard.subtitle` is now empty in both locales (the prior placeholder text added no information). "Time bucket" is renamed to "Time interval" in chart subtitles — "bucket" is engineer-speak, "interval" is what the number actually means.
- **Removed spurious `users.rs` / `app_state.rs` tests** added by the rename agent during the dispatch → routing refactor.

#### Changed

- **`dispatch` renamed to `routing` across the whole codebase.** Pure mechanical rename at every layer — same semantics, clearer name:
  - **SDK** (`gproxy-channel`, `gproxy-engine`): `DispatchTable` → `RoutingTable`, `DispatchTableDocument` → `RoutingTableDocument`, `DispatchTableError` → `RoutingTableError`, `DispatchRuleDocument` → `RoutingRuleDocument`, `Channel::dispatch_table()` → `Channel::routing_table()`, `ProviderRuntime::dispatch_table()` → `routing_table()`, `ProviderStore::get_dispatch_table()` → `get_routing_table()`, `add_provider_with_dispatch()` → `add_provider_with_routing()`, `ProviderConfig.dispatch` → `routing`, `dispatch.rs` → `routing.rs`, `dispatch_alignment.rs` → `routing_alignment.rs`. `gproxy_protocol::transform::dispatch` (separate runtime-keyed transform dispatcher) is intentionally untouched.
  - **API + storage**: field and column rename across admin, providers, bootstrap, handler, store-mutation, store-query, write-sink, write-event, entities, and query layers. A sea-orm-migration `m20260416_000001_rename_dispatch_to_routing` renames the `providers.dispatch_json` column to `providers.routing_json` before schema sync — idempotent, skipped on fresh DBs, and ledger-recorded so it runs at most once per DB.
  - **Frontend console**: hook, module, type, and i18n strings renamed; `dispatch.ts` / `dispatch.test.ts` → `routing.ts` / `routing.test.ts`.
  - **Docs**: `docs/src/content/docs/reference/dispatch-table.md` and its zh-cn counterpart moved to `routing-table.md`; README, Astro sidebar, guides, and architecture docs updated.
- **Dashboard credential health replaced from table to grouped summary counts.** The old per-credential rows (provider / index / status / available) are replaced by per-provider summary chips showing `healthy / cooldown / dead` counts, so each channel's status is visible at a glance without scrolling a long table.
- **Redundant inline migration removed.** The `dispatch_json → routing_json` rename briefly had two implementations (raw-SQL inline `migrations.rs` + sea-orm-migration). The inline one is deleted; sea-orm-migration is the single source of truth.

#### Compatibility

- **Drop-in upgrade** from v1.0.11. No HTTP API change, no config change at the surface level.
- **DB migration**: `providers.dispatch_json` is renamed to `providers.routing_json` via sea-orm-migration on startup. Idempotent; safe on fresh and migrated DBs. Rollback is supported via `down()`.
- **SDK rename is a breaking change for direct SDK consumers.** Code that imports `DispatchTable`, calls `Channel::dispatch_table()`, or constructs `ProviderConfig { dispatch: … }` must rename to the `routing` variant. The gproxy binary and console are unaffected.
- **Existing pricing JSON with `i64::MAX` upper bound**: backend accepts the value, but the console now clamps user input to `MAX_SAFE_INTEGER` and the built-in templates use `100_000_000`. Existing rows keep working; re-saving a tier via the UI will clamp it.

### 简体中文

#### 新增

- **Models 标签新增「应用默认定价」按钮。** 后端 12 个 per-channel pricing JSON 文件(共 397 个模型)合并进前端查找表 `frontend/console/src/lib/default-model-pricing.ts`。每个模型的编辑表单新增一个按钮,以 `model_id` 最后一段(`/` 之后)对模板做最长子串匹配,一键填充 `pricing_json`——模板里认识的模型都能一键完成定价配置。
- **OpenAI Responses WebSocket 鉴权探测.** 当 operation 是 `OpenAiResponseWebSocket` 时,`UpstreamWebSocket` 在连接后等待 150ms 的首帧;若判定为 401/403 / `invalid_api_key` / permission / unauthorized 之类的鉴权错误,就把该 credential 标死,engine 切换到下一个。非鉴权的首帧会被 buffer,首次 `recv()` 时原样交付,下游看不到任何数据丢失。此前一个错的 `sk-proj-…` 会得到成功的 `101 Switching Protocols`、立即出错帧、用户侧报错、credential 不轮换。
- **`prepare_ws_auth` 返回 credential 下标并做 round-robin 排序.** WS 鉴权候选的元组从 `(url, headers)` 改为 `(credential_index, url, headers)`。runtime 先基于 cooldown-health 过滤掉死 credential,然后用一个原子游标轮询起始偏移,和 HTTP 执行逻辑对齐。
- **`parseBetaHeaders` 支持 JSON 数组字符串.** 旧的 CSV 输入(`"a,b,c"`)替换为严格的 JSON 数组解析(`'["a","b","c"]'`),让 `BetaHeadersEditor` 能无歧义地往返结构化配置。非法输入返回 `[]` 而不是悄悄地部分解析。新增单测覆盖。

#### 修复

- **代理响应头规范化.** 新增的 `normalize_response_headers` helper 会从 `proxy`、`proxy_unscoped`、`proxy_unscoped_files` 的每个响应中剥离 3 个上游相关的 header(`Content-Length`、`Content-Encoding`、`Transfer-Encoding`)——body 经过 axum 重新 stream 后这些过期值会破坏 chunked 编码 / gzip 链路。当上游完全没发 `Content-Type` 时,按 (operation, protocol) 组合注入正确默认值——Claude / OpenAI-chat / OpenAI-response / Gemini 流式用 `text/event-stream`,非流式生成 / count-token / compact / embedding / image / file / model-list 路由用 `application/json`。
- **`OpenAiResponseToClaudeStream` 不再重复输出已关闭 block.** 转换器新增一组 per-block 集合(`completed_text_blocks` / `completed_thinking_blocks` / `completed_summary_blocks` / `streamed_message_items` / `streamed_tool_args`),保证:流式 delta 之后到来的 `*.done` 事件对已打开的 block 只发一次关闭;与流式 `function_call_arguments.done` 相同 `item_id` 的工具调用 `output_item.done` 只产生一次 `content_block_stop`,不再重开 block。重写时把多处重复的 per-event block 关闭逻辑统一到 `finish_text_block` / `finish_thinking_block` / `finish_summary_block`。
- **`OpenAiResponseToClaudeStream` 保留 `tool_use` stop 原因.** 当 `ResponseStreamEvent::Completed` 不带 `incomplete_details.reason` 时,转换器之前强制 `stop_reason = BetaStopReason::EndTurn`,这会覆盖工具调用映射器设置的 `ToolUse`。现在这个分支把 `stop_reason` 留空(`None`),让工具驱动的 stop 原因传播到最终的 `message_delta`。新增回归测试:function-call → completed 序列断言 `BetaStopReason::ToolUse`。
- **Pricing 保存修复:缺失 `model_id` 与 i64 溢出.** `ModelPrice.model_id` 加 `#[serde(default)]`,因为前端不发这个字段(后端从 URL 参数覆写),之前硬性要求导致保存报 400。Pricing 模板里「无上限」的分层上限从 `i64::MAX`(`9_223_372_036_854_775_807`)下调为 `100_000_000`——JavaScript `JSON.parse` 会把 `i64::MAX` 舍入成 `9_223_372_036_854_776_000`,往返就溢出 i64。100M tokens 仍然等同无上限(没有 LLM 的上下文窗口接近这个数量级)。
- **Dashboard i18n.** `dashboard.subtitle` 在中英两种语言下都清空(之前的占位文本没带任何信息)。图表副标题里的 "Time bucket" 改为 "Time interval"——"bucket" 是工程师黑话,"interval" 才是那个数字的真实含义。
- **清理 rename agent 误加的 `users.rs` / `app_state.rs` 测试**(dispatch → routing 重构过程中遗留)。

#### 变更

- **全代码库 `dispatch` 改名为 `routing`.** 纯机械改名,语义不变,但语义更清晰:
  - **SDK** (`gproxy-channel`、`gproxy-engine`):`DispatchTable` → `RoutingTable`、`DispatchTableDocument` → `RoutingTableDocument`、`DispatchTableError` → `RoutingTableError`、`DispatchRuleDocument` → `RoutingRuleDocument`、`Channel::dispatch_table()` → `Channel::routing_table()`、`ProviderRuntime::dispatch_table()` → `routing_table()`、`ProviderStore::get_dispatch_table()` → `get_routing_table()`、`add_provider_with_dispatch()` → `add_provider_with_routing()`、`ProviderConfig.dispatch` → `routing`、`dispatch.rs` → `routing.rs`、`dispatch_alignment.rs` → `routing_alignment.rs`。`gproxy_protocol::transform::dispatch`(独立的 runtime-keyed transform 分发器)刻意保持不变。
  - **API + storage**:字段和列名在 admin、providers、bootstrap、handler、store-mutation、store-query、write-sink、write-event、entities、query 各层统一改名。新增 sea-orm-migration `m20260416_000001_rename_dispatch_to_routing`,在 schema sync 之前把 `providers.dispatch_json` 列重命名为 `providers.routing_json`——幂等、新 DB 跳过、有 ledger 记录保证每个 DB 最多执行一次。
  - **前端控制台**:hook、module、type、i18n 字符串统一改名;`dispatch.ts` / `dispatch.test.ts` → `routing.ts` / `routing.test.ts`。
  - **文档**:`docs/src/content/docs/reference/dispatch-table.md` 与其中文版迁移为 `routing-table.md`;README、Astro 侧边栏、guides、架构文档一并更新。
- **Dashboard credential health 从表格改为分组汇总.** 原本按 credential 逐行展示(provider / index / status / available)被替换为按 provider 分组的 `healthy / cooldown / dead` 计数 chip,一眼就能看到每个 channel 的状态,不再需要滚动长表。
- **移除冗余的 inline migration.** `dispatch_json → routing_json` 重命名短暂出现过两套实现(原始 SQL 的 inline `migrations.rs` + sea-orm-migration)。inline 那份删除,保留 sea-orm-migration 作为单一真源。

#### 兼容性

- **从 v1.0.11 直接升级**。HTTP API 表层无变化,配置表层无变化。
- **DB 迁移**:启动时 sea-orm-migration 自动把 `providers.dispatch_json` 重命名为 `providers.routing_json`。幂等;新库和已迁移的库都安全。支持通过 `down()` 回滚。
- **SDK 改名对直接使用 SDK 的调用方是破坏性变更**。import `DispatchTable`、调用 `Channel::dispatch_table()`、构造 `ProviderConfig { dispatch: … }` 的代码需要改成 `routing` 命名。gproxy 二进制和控制台不受影响。
- **已有 pricing JSON 里 `i64::MAX` 上限的行**:后端接受该值,但控制台现在会把用户输入 clamp 到 `MAX_SAFE_INTEGER`,内置模板改用 `100_000_000`。已有行继续可用;通过 UI 重新保存某个 tier 会 clamp。

## v1.0.11

> End-to-end upstream latency tracking (TTFB + total) from transport layer to DB to console, a new dashboard module with credential health / KPI / traffic charts, protocol-aware auth for custom channel dispatch routes, and a LogGuard that finally flushes request logs on panic and stream cancel.

### English

#### Added

- **Upstream latency tracking end-to-end.** The transport layer now captures TTFB (`initial_latency_ms`) and total request duration (`total_latency_ms`) on every upstream response. The engine propagates both through `UpstreamRequestMeta`, the handler persists them as two new nullable `BIGINT` columns on the `upstream_requests` table (applied by `schema.sync()` on startup; legacy rows keep `NULL`), and the console's requests table renders them as a single "Latency" column showing `120ms / 3.4s` format — ms under 1s, seconds with one decimal above, `–` for missing halves. The old ambiguous single `latency_ms` field in the engine meta is replaced by the two explicit fields; the dead `send_start` timer in `retry.rs` is removed since each attempt's timings now come from the response directly.
- **Dashboard module.** New `/console#dashboard` view with a `CredentialHealthPanel` (per-credential status breakdown), `KpiCards` (key performance indicators), `TrafficChart` and `StatusCodesChart` (time-series visualizations), `TopProvidersTable` and `TopModelsTable` (ranked usage). State is managed via a `useDashboardState` hook that fetches from the admin API. Includes unit tests for dashboard state helpers.
- **Console hash-based module routing.** Root redirect now points at `/console` instead of `/console/login`. Valid `#<moduleId>` hashes (e.g. `/console#users`, `/console#requests`) open that module directly on load; Nav clicks push the matching hash so browser back/forward step through visited modules. Unknown or role-forbidden hashes are stripped from the URL so the address bar always matches what's rendered. Logout clears the hash.
- **Cloudflare header stripping.** The sanitize middleware now strips Cloudflare-injected headers before forwarding to upstream, preventing leaked infrastructure headers on proxied requests.

#### Fixed

- **Request log flushed on panic and stream cancel.** The DB write is now wrapped in a `LogGuard` whose `Drop` impl spawns the record task. Three previously-silent cases now produce log entries: a panic in the middleware body, an SSE stream cancelled by client disconnect, and an SSE stream that errors mid-flight. Partial state is written with `status = None` when the response line was never observed.
- **Custom channel protocol-aware auth headers.** The custom channel's `prepare_request` previously used `settings.auth_scheme` (default: bearer) for every route, which silently broke any dispatch that xformed into Claude or Gemini — e.g. a custom provider pointing at `api.anthropic.com` with the anthropic-like dispatch template would get a Bearer header, Anthropic returns 401, the engine marks the credential dead, and `/admin/models/pull` reports "all credentials exhausted" even with a valid `sk-ant-...` key. Now: Claude routes send `x-api-key` + `anthropic-version: 2023-06-01`, Gemini/GeminiNDJson routes send `x-goog-api-key`, OpenAI-family routes keep Bearer. The `auth_scheme` config field is dropped entirely (see Changed).
- **`pull_models` xform body.** The admin pull_models refactor passed `body=Vec::new()` on the assumption that ModelList only flows through Passthrough or Local routes. That breaks user-defined dispatch overrides (e.g. a custom channel using the anthropic-like template, which routes through xform). The transformer calls `serde_json::from_slice::<RequestBody>(body)` and an empty buffer fails with "EOF while parsing". Sending `{}` fixes xform routes; Passthrough routes still get a valid payload that every upstream ignores.
- **`model_list` body shim dropped.** `build_live_model_list_request_body` built `{"query":{"limit":1000}}` as the request body for live model listing, under the misconception that this would propagate pagination params. It did not — Claude/Gemini `QueryParameters` are URL query params, not JSON body fields; the transformer for xform routes silently dropped the `query` key; and stricter upstream proxies echoed the opaque blob downstream, confusing operators. Replaced with `b"{}".to_vec()`.
- **`cache_creation` extracted from `iterations` in `message_delta`.** The Claude API nests the `cache_creation` object (with `ephemeral_5m/1h_input_tokens`) inside `usage.iterations[0]` in `message_delta` events, not directly under `usage`. Now falls back to `iterations[0].cache_creation` when `usage.cache_creation` is absent.
- **ClaudeCodeChannel session ID management.** Improved session ID lifecycle and caching to prevent stale session references.
- **Channel-managed request headers no longer duplicate caller-supplied values.** Provider-auth, content-type, user-agent, and other channel-owned headers are now written as final replacements so proxied requests do not carry duplicate `Authorization` / `User-Agent` / `Content-Type`-style entries when the caller already sent them.
- **Codex cached token usage preserved.** Token usage from cached responses is no longer silently dropped.
- **Console i18n.** `table.latency` translated as 延迟 (latency) instead of 耗时 (duration).

#### Changed

- **Custom channel drops `auth_scheme` field.** The field was added in d7691681 as a configurable switch for bearer / x-api-key / query-key, but the frontend form never exposed it and no user could set it without hand-editing `settings_json`. After protocol-aware auth headers (see Fixed), `auth_scheme` had no reachable effect. `prepare_request` now picks headers purely from `request.route.protocol`. Backward compat: `CustomSettings` has no `deny_unknown_fields`, so existing rows containing `"auth_scheme": "..."` deserialize unchanged (the field is silently dropped).
- **Admin `pull_models` unified to OpenAI protocol.** Drops the per-channel protocol mapping. Every channel already registers `(ModelList, OpenAi)` in its routing table — as passthrough, xform, or local — so a single OpenAi `execute` call lets the routing layer handle protocol conversion. Removes `channel_to_model_list_protocol`, `build_live_model_list_request_body`, and the Claude/Gemini branches of `extract_model_ids`. Net −66 lines.
- **Console module restructuring.** `ProvidersModule.tsx` (932 → 303 lines) split into `CredentialsPane`, `ModelsPane`, and `OAuthPane` container components, each owning their own state and handlers. `SettingsEditors.tsx` split into `settings-editors/` with one file per editor. Extracted `SuffixVariantDialog`, `usePullModelsPanel` hook, and `RewriteRuleEditor` into standalone files. Dropped unused `RewriteRulesEditor` definitions. Pure restructure; no behaviour change.

#### Compatibility

- **Drop-in upgrade** from v1.0.10. No HTTP API change, no config change. SDK consumers are unaffected — no public types or module paths moved.
- **DB migration**: two nullable `BIGINT` columns (`initial_latency_ms`, `total_latency_ms`) added to `upstream_requests` via `schema.sync()` on startup. Additive only; legacy rows keep `NULL`. No manual migration step required.
- **Custom channel `auth_scheme`**: silently ignored if present in existing `settings_json` rows — no breakage, no manual cleanup needed.

### 简体中文

#### 新增

- **上游延迟端到端追踪.** transport 层捕获每个上游响应的 TTFB (`initial_latency_ms`) 和总耗时 (`total_latency_ms`)。engine 通过 `UpstreamRequestMeta` 透传,handler 持久化为 `upstream_requests` 表的两个新 nullable `BIGINT` 列(启动时 `schema.sync()` 自动加字段;旧行保持 `NULL`)。控制台请求表渲染为一列 "延迟",格式 `120ms / 3.4s` —— 1s 以下用 ms,1s 以上用一位小数的 s,缺值显示 `–`。engine meta 里原来含义模糊的单 `latency_ms` 字段替换为这两个明确字段;`retry.rs` 里已废弃的 `send_start` timer 删除,因为每次尝试的耗时现在直接从响应获取。
- **Dashboard 模块.** 新增 `/console#dashboard` 视图,包含 `CredentialHealthPanel`(每 credential 状态分布)、`KpiCards`(关键性能指标)、`TrafficChart` / `StatusCodesChart`(时序可视化)、`TopProvidersTable` / `TopModelsTable`(按用量排名)。状态通过 `useDashboardState` hook 管理,从 admin API 拉取数据。附带 dashboard state helper 单测。
- **控制台 hash 路由.** 根跳转目标从 `/console/login` 改为 `/console`。有效的 `#<moduleId>` hash(如 `/console#users`、`/console#requests`)在加载时直接打开对应模块;Nav 点击推入对应 hash,浏览器前进/后退可在已访问模块间切换。无效或角色不可访问的 hash 会从 URL 中剥离,保证地址栏与渲染始终一致。登出清空 hash。
- **Cloudflare header 剥离.** sanitize 中间件在转发上游前剥离 Cloudflare 注入的 header,防止基础设施 header 泄漏到代理请求中。

#### 修复

- **panic 和流取消时刷写请求日志.** DB 写入包裹在 `LogGuard` 里,`Drop` impl 负责 spawn 写入任务。三种之前静默丢失的场景现在都产生日志:中间件 body 里 panic、客户端断开导致 SSE 流取消、SSE 流在传输中出错。未观察到响应行时,以 `status = None` 写入部分状态。
- **Custom channel 协议感知 auth header.** custom channel 的 `prepare_request` 之前对所有 route 统一用 `settings.auth_scheme`(默认 bearer),这会静默破坏任何 xform 到 Claude 或 Gemini 的 dispatch —— 比如一个 base_url 指向 `api.anthropic.com` 并使用 anthropic-like dispatch 模板的 custom provider,Bearer header 导致 Anthropic 返回 401,engine 把 credential 标死,`/admin/models/pull` 报 "all credentials exhausted"。修复后:Claude route 发 `x-api-key` + `anthropic-version: 2023-06-01`,Gemini/GeminiNDJson route 发 `x-goog-api-key`,OpenAI 族 route 保持 Bearer。`auth_scheme` 配置字段整体删除(见变更)。
- **`pull_models` xform body.** admin pull_models 重构传了 `body=Vec::new()`,假设 ModelList 只走 Passthrough 或 Local route。用户自定义 dispatch 覆盖(如 anthropic-like 模板走 xform)会因为空 buffer 在 `serde_json::from_slice::<RequestBody>` 处 EOF 解析失败。改发 `{}`。
- **`model_list` body shim 移除.** `build_live_model_list_request_body` 构造 `{"query":{"limit":1000}}` 作为实时模型列表请求 body,以为能传递分页参数。实际没用 —— Claude/Gemini 的 `QueryParameters` 是 URL 查询参数不是 JSON body 字段;xform route 的 transformer 悄悄丢掉 `query` key;更严格的上游代理(如 gptload → newapi)会原样回传这坨不明 blob,搞晕运维。替换为 `b"{}".to_vec()`。
- **`message_delta` 中的 `cache_creation` 提取.** Claude API 把 `cache_creation` 对象(含 `ephemeral_5m/1h_input_tokens`)嵌套在 `message_delta` 事件的 `usage.iterations[0]` 里,而非直接放在 `usage` 下。现在 `usage.cache_creation` 缺失时回退到 `iterations[0].cache_creation`。
- **ClaudeCodeChannel session ID 管理.** 改善了 session ID 的生命周期和缓存,防止过期 session 引用。
- **channel 自管请求头不再和调用方重复.** provider 鉴权、content-type、user-agent 等由 channel 负责的 header 现在会在最后做覆盖写入,避免调用方已携带这些字段时,代理后的请求再出现重复的 `Authorization` / `User-Agent` / `Content-Type` 一类条目。
- **Codex cached token usage 保留.** 缓存响应中的 token 用量不再被静默丢弃。
- **控制台 i18n.** `table.latency` 翻译为"延迟"而非"耗时"。

#### 变更

- **Custom channel 移除 `auth_scheme` 字段.** 该字段在 d7691681 加入,可配置 bearer / x-api-key / query-key,但前端表单从未暴露,用户只有手改 `settings_json` 才能设置。协议感知 auth header 修复后 `auth_scheme` 不再有可达效果。`prepare_request` 现在纯粹从 `request.route.protocol` 决定 header。向后兼容:`CustomSettings` 没有 `deny_unknown_fields`,已有的 `"auth_scheme": "..."` 行反序列化不变(字段被静默忽略)。
- **Admin `pull_models` 统一为 OpenAI 协议.** 移除 channel→protocol 映射。每个 channel 的 routing 表已经注册了 `(ModelList, OpenAi)` —— passthrough、xform 或 local —— 所以一次 OpenAi `execute` 调用让 routing 层处理协议转换。移除 `channel_to_model_list_protocol`、`build_live_model_list_request_body` 和 `extract_model_ids` 的 Claude/Gemini 分支。净减 66 行。
- **控制台模块重构.** `ProvidersModule.tsx`(932 → 303 行)拆分为 `CredentialsPane`、`ModelsPane`、`OAuthPane` 容器组件,各自管理自己的状态和 handler。`SettingsEditors.tsx` 拆到 `settings-editors/` 目录,每个编辑器一个文件。提取 `SuffixVariantDialog`、`usePullModelsPanel` hook、`RewriteRuleEditor` 为独立文件。删除已无人使用的 `RewriteRulesEditor` 定义。纯结构重组,无行为变更。

#### 兼容性

- **从 v1.0.10 直接升级**。不涉及 HTTP API 变更或配置变更。SDK 使用者不受影响 —— 没有任何公开类型或模块路径移动。
- **DB 迁移**:`upstream_requests` 表新增两个 nullable `BIGINT` 列(`initial_latency_ms`、`total_latency_ms`),启动时 `schema.sync()` 自动执行。纯增量;旧行保持 `NULL`。无需手动迁移。
- **Custom channel `auth_scheme`**:已有 `settings_json` 行中的该字段被静默忽略 —— 不会中断,无需手动清理。

## v1.0.10

> Two focused fixes from the v1.0.9 fallout: claudecode OAuth refresh was broken against Anthropic's token endpoint and left credentials permanently dead, and the sanitize middleware was leaking `anthropic-version` through so every upstream request carried a duplicated header.

### English

#### Fixed

- **claudecode OAuth refresh actually works again.** The v1.0.9 gproxy-channel refactor routed `refresh_credential`'s `refresh_token` path through the generic `oauth2_refresh::refresh_oauth2_token` helper, which posts `grant_type=refresh_token&refresh_token=...` (no `client_id`, no anthropic headers) to `https://console.anthropic.com/v1/oauth/token`. Anthropic's token endpoint rejects that shape with `invalid_request_error: Invalid request format`, so any credential with a `refresh_token` but no cookie fallback was stuck dead forever — the 401 → refresh → retry loop would fail every time. Replaced with `exchange_tokens_with_refresh_token` in `claudecode_cookie.rs`, which posts the CLI-matching shape to `{api_base}/v1/oauth/token` (form body with `client_id=9d1c250a-...` and headers `anthropic-version: 2023-06-01` / `anthropic-beta: oauth-2025-04-20` / `user-agent: claude-cli/...`).
- **Pre-flight credential refresh.** Added `Channel::needs_refresh` as a new trait hook (default `false`). claudecode overrides it to return `true` when `access_token` is empty, `expires_at_ms` is already past, or expiry is within a 60s skew window. The retry loop now calls `refresh_credential` up-front for such credentials and proceeds with the fresh token, skipping the otherwise-guaranteed 401 round-trip. Errors from the pre-flight are logged and swallowed — the existing AuthDead path still catches anything that slips through.
- **`anthropic-version` no longer duplicated on upstream requests.** The request sanitize middleware's `HEADER_DENYLIST` was already stripping `authorization` / `user-agent` / `content-type` / etc. from the downstream request before the channel forwarding loop ran — but `anthropic-version` was missing from the list. Since `http::request::Builder::header` *appends* rather than replaces, the client-forwarded copy ended up alongside the channel's own value, producing `anthropic-version: 2023-06-01` twice on the wire. Added to the denylist.

#### Compatibility

- **Drop-in upgrade** from v1.0.9. No DB migration, no HTTP API change, no config change. SDK consumers are unaffected — no public types or module paths moved.

### 简体中文

#### 修复

- **claudecode OAuth refresh 重新可用.** v1.0.9 的 gproxy-channel 重构把 `refresh_credential` 的 `refresh_token` 路径切到通用的 `oauth2_refresh::refresh_oauth2_token` helper,它往 `https://console.anthropic.com/v1/oauth/token` POST `grant_type=refresh_token&refresh_token=...`(没有 `client_id`,没有 anthropic header),Anthropic 的 token 端点会返回 `invalid_request_error: Invalid request format` 直接拒绝,所以只有 `refresh_token` 没有 cookie 兜底的 credential 永远死透 —— 401 → refresh → retry 循环每次都失败。换成 `claudecode_cookie.rs` 里新增的 `exchange_tokens_with_refresh_token`,按 CLI 的请求 shape 打到 `{api_base}/v1/oauth/token`(form body 带 `client_id=9d1c250a-...`,header 带 `anthropic-version: 2023-06-01` / `anthropic-beta: oauth-2025-04-20` / `user-agent: claude-cli/...`)。
- **Credential 的 pre-flight refresh.** 新增 `Channel::needs_refresh` trait 方法(默认 `false`)。claudecode 覆盖实现:`access_token` 为空、`expires_at_ms` 已经过期、或 60 秒内即将过期时返回 `true`。retry 循环检测到后先调用 `refresh_credential` 刷新一次再发请求,省掉那次必然 401 的 round-trip。pre-flight 报错只记日志不中断,现有的 AuthDead 回退路径继续兜底。
- **`anthropic-version` 不再在上游请求中重复.** 请求 sanitize 中间件的 `HEADER_DENYLIST` 之前已经在进 channel 转发循环之前抹掉了 `authorization` / `user-agent` / `content-type` 等,但漏了 `anthropic-version`。由于 `http::request::Builder::header` 是 *追加* 而不是替换,客户端发来的那份会和 channel 自己设的那份一起出现,上游就看到两份 `anthropic-version: 2023-06-01`。已加进 denylist。

#### 兼容性

- **从 v1.0.9 直接升级**。不涉及 DB 迁移、HTTP API 变更或配置变更。SDK 使用者不受影响 —— 没有任何公开类型或模块路径移动。

## v1.0.9

> The SDK splits into four publishable crates — `gproxy-protocol`, `gproxy-channel`, `gproxy-engine`, `gproxy-sdk` — with real per-channel feature pruning, a standalone `execute_once` single-request client for single-provider use, and no DB / API / config changes for binary operators.

### English

#### Added

- **Four publishable SDK crates** — `gproxy-protocol` (L0 wire types + transforms), `gproxy-channel` (L1 `Channel` trait, 14 concrete channels, credentials, `execute_once` pipeline), `gproxy-engine` (L2 `GproxyEngine`, provider store, retry, affinity, routing helpers), and `gproxy-sdk` (facade re-exporting all three). Every SDK crate now carries complete crates.io metadata (license, readme, keywords, categories) and a per-crate README with a common layering table.
- **`execute_once` / `execute_once_stream`** in `gproxy_channel::executor` — a complete single-request pipeline (finalize → sanitize → rewrite → prepare_request → HTTP send → normalize → classify) you can drive with just `gproxy-channel` as a dependency. Comes with lower-level `prepare_for_send` / `send_attempt` / `send_attempt_stream` helpers for users who want to write their own retry loop.
- **`apply_outgoing_rules` helper** — the single in-tree invocation point for `apply_sanitize_rules` + `apply_rewrite_rules`. Engine, API handler, and L1 executor all funnel through one body-mutation helper instead of each re-implementing the JSON round-trip.
- **`CommonChannelSettings`** (`#[serde(flatten)]`) — every channel now embeds one common struct holding `user_agent`, `max_retries_on_429`, `sanitize_rules`, `rewrite_rules` instead of each of the 14 channels copy-pasting the same four fields and trait method overrides. TOML / JSON wire format is unchanged.
- **Runtime transform dispatcher as public L0 API** — `gproxy_protocol::transform::dispatch::{transform_request, transform_response, create_stream_response_transformer, nonstream_to_stream, stream_to_nonstream, convert_error_body_or_raw}`. External users who only want protocol conversion can now depend on `gproxy-protocol` alone and get everything without pulling `wreq` or `tokio`.
- **`hello_openai` example** in `sdk/gproxy-channel/examples/` — a minimal single-file demo of `execute_once` that runs against real OpenAI with `OPENAI_API_KEY`. Compiles under `--no-default-features --features openai` as a smoke test that single-channel use really only pulls one channel.
- **Integration test for `execute_once`** — spins up a local `axum` mock server, points `OpenAiSettings::base_url` at it, runs the full L1 pipeline, and asserts on both request side (Bearer token, body) and response side (status, classification, JSON).
- **Optional `label` field on provider** — free-text display name shown in the console alongside the internal provider name.

#### Changed

- **`TransformError` now carries `Cow<'static, str>` messages** so the runtime dispatcher can produce dynamically-built errors (`format!("no stream aggregation for protocol: {protocol}")`) without allocating a new `TransformError` variant. Existing `TransformError::not_implemented("literal")` call sites keep working; new `TransformError::new(impl Into<String>)` constructor handles the dynamic case.
- **`store.rs` split** — the 1564-line `gproxy-engine/src/store.rs` is now `store/{mod,public_traits,runtime,types}.rs` so the main `ProviderStore` orchestrator, the internal `ProviderRuntime` trait + `ProviderInstance<C>` generic implementation, the public traits, and the value types each live in their own file.
- **Lock-step SDK versioning** — all four SDK crates follow `workspace.package.version`; `release.sh`'s `cargo set-version` bump propagates to every `[package]` inherit plus the four `workspace.dependencies.gproxy-*.version` entries at once. The release strategy + manual publish recipe is documented inline in the root `Cargo.toml`.

#### Fixed

- **Per-channel feature flags now actually prune** — the `openai`, `anthropic`, … channel feature flags on `gproxy-channel`, `gproxy-engine`, and `gproxy-sdk` were declared in v1.0.8 but non-functional. `cargo build --no-default-features --features openai` compiled all 14 channels anyway, because (a) the upstream `gproxy-channel` dep didn't opt out of default-features, so the default `all-channels` came in regardless; (b) `gproxy-engine`'s `all-channels` feature only forwarded to `gproxy-channel/all-channels` and didn't enable its own per-channel features, so the `#[cfg(feature = "…")]` gates would have been false even if they existed; and (c) the gates didn't exist on engine's hardcoded match arms in `built_in_model_prices`, `validate_credential_json`, `GproxyEngineBuilder::add_provider_json`, `ProviderStore::add_provider_json`, and `bootstrap_credential_on_upsert`. All three fixed in this release, and `cargo build -p gproxy-sdk --no-default-features --features openai` now genuinely compiles only the single requested channel.
- **Pricing editor in the console** collapses into a single triangle disclosure — the nested editor no longer cascades open by accident.
- **Dispatch template description** now clarifies that it describes the upstream protocol, not the downstream-client shape.
- **Claude Code OAuth beta badge** drops the misleading "always" suffix; the badge just shows the beta name now.
- **Self-update button** and its success toast are now localized.
- **Doc-comment clippy lint** (`doc_lazy_continuation`) on `gproxy-engine` crate doc no longer fails `cargo clippy -- -D warnings`.

#### Removed

- **`gproxy-provider` crate** — the old aggregator that mixed single-channel access with the multi-channel engine. Its content is now split between `gproxy-channel` (L1) and `gproxy-engine` (L2).
- **`gproxy-routing` crate** — merged into `gproxy-engine::routing` (`classify`, `permission`, `rate_limit`, `provider_prefix`, `model_alias`, `model_extraction`, `headers` / former `sanitize.rs`).
- **Deprecated `gproxy_sdk::provider` / `gproxy_sdk::routing` module aliases** — use `gproxy_sdk::channel::*`, `gproxy_sdk::engine::*`, `gproxy_sdk::engine::routing::*` instead.
- **Unused `ProviderDefinition` type** — dead code with no consumers.
- **`gproxy-engine::transform_dispatch` passthrough** — engine now calls `gproxy_protocol::transform::dispatch::*` directly; the 14-line re-export file is gone.

#### Compatibility

- **Binary / server operators**: drop-in upgrade from v1.0.8. No DB migration, no HTTP API change, no admin client change, no config change.
- **SDK library consumers**: breaking change. `gproxy_sdk::provider::*` and `gproxy_sdk::routing::*` paths no longer exist. Migrate every import site to `gproxy_sdk::channel::*`, `gproxy_sdk::engine::*`, `gproxy_sdk::engine::routing::*` (for the former routing helpers), or `gproxy_sdk::protocol::transform::dispatch::*` (for the runtime transform dispatcher). All in-tree downstream consumers have already been migrated.
- **Direct `gproxy-provider` / `gproxy-routing` dependencies** in downstream `Cargo.toml` must be replaced with `gproxy-channel` + `gproxy-engine`, or just `gproxy-sdk` if you want the facade.
- **14 channel `Settings` structs** gained a `common: CommonChannelSettings` field flattened via serde, so existing TOML / JSON configs deserialize unchanged.
- **crates.io publishing**: The four SDK crates are metadata-complete and packaged (verified via `cargo publish --dry-run` on `gproxy-protocol` and `cargo package --list` on the downstream three). Actual publish has NOT happened yet — this release is local to the repo. When you publish, the dependency order is `gproxy-protocol → gproxy-channel → gproxy-engine → gproxy-sdk` with ~30 s between each step for the registry index to catch up.

### 简体中文

#### 新增

- **四个可发布的 SDK crate** — `gproxy-protocol`(L0 wire 类型 + 协议转换)、`gproxy-channel`(L1 `Channel` trait、14 个具体 channel、credentials、`execute_once` 流水线)、`gproxy-engine`(L2 `GproxyEngine`、provider store、retry、affinity、路由 helper),以及 `gproxy-sdk`(facade,重导出上述三个)。每个 crate 都带齐 crates.io 元数据(license、readme、keywords、categories)和独立 README,README 顶部有统一的分层对照表。
- **`execute_once` / `execute_once_stream`**(在 `gproxy_channel::executor`)—— 单次请求完整流水线(finalize → sanitize → rewrite → prepare_request → HTTP send → normalize → classify),只依赖 `gproxy-channel` 就能跑。还附带 `prepare_for_send` / `send_attempt` / `send_attempt_stream` 低阶 helper,供需要自己写 retry 循环的用户使用。
- **`apply_outgoing_rules` helper** —— `apply_sanitize_rules` + `apply_rewrite_rules` 在仓库内的唯一调用点。engine、API handler 和 L1 executor 全部通过一个 body 变换 helper 走,不再各自重复 JSON 反序列化 / 变换 / 序列化三部曲。
- **`CommonChannelSettings`**(`#[serde(flatten)]`)—— 14 个 channel 的 `Settings` struct 现在统一 embed 一个 common struct,里面装 `user_agent`、`max_retries_on_429`、`sanitize_rules`、`rewrite_rules`,不再各自 copy-paste 同样的四个字段和四个 trait 方法。TOML / JSON 线格式不变。
- **运行时协议分发作为 L0 公开 API** —— `gproxy_protocol::transform::dispatch::{transform_request, transform_response, create_stream_response_transformer, nonstream_to_stream, stream_to_nonstream, convert_error_body_or_raw}`。只想做协议转换的外部用户现在只依赖 `gproxy-protocol` 就够了,不会被 `wreq`、`tokio` 拖进来。
- **`hello_openai` 示例**(`sdk/gproxy-channel/examples/`)—— 用 `OPENAI_API_KEY` 打真实 OpenAI 的单文件 demo。用 `--no-default-features --features openai` 编译就能作为"单渠道场景真的只拖一家"的 smoke test。
- **`execute_once` 集成测试** —— 起本地 `axum` mock 服务,把 `OpenAiSettings::base_url` 指过去,跑完整 L1 流水线,从请求侧(Bearer token、body)和响应侧(status、classification、JSON)双向断言。
- **provider 新增可选 `label` 字段** —— 控制台里显示的自由文本名称,与内部 provider 名称并列。

#### 变更

- **`TransformError` 消息改为 `Cow<'static, str>`**,让运行时 dispatcher 能动态构造错误(`format!("no stream aggregation for protocol: {protocol}")`),不用为此新增 `TransformError` 变体。旧的 `TransformError::not_implemented("literal")` 调用位照旧工作;新的 `TransformError::new(impl Into<String>)` 构造器负责动态场景。
- **`store.rs` 拆分** —— 原本 1564 行的 `gproxy-engine/src/store.rs` 拆成 `store/{mod,public_traits,runtime,types}.rs`,主 `ProviderStore` 编排层、内部 `ProviderRuntime` trait + `ProviderInstance<C>` 泛型实现、公开 trait、值类型各自独立成文件。
- **SDK 锁步版本** —— 四个 SDK crate 统一跟随 `workspace.package.version`;`release.sh` 里的 `cargo set-version` 会把 bump 一次性同步到所有 `[package] version.workspace = true` 继承位,以及 `workspace.dependencies.gproxy-*.version` 四条内部依赖版本。发版策略和手动发布 recipe 写在根 `Cargo.toml` 顶部的注释块里。

#### 修复

- **per-channel feature flag 真正裁剪** —— v1.0.8 里 `openai`、`anthropic`、... 这些渠道 feature 虽然在 `gproxy-channel`、`gproxy-engine`、`gproxy-sdk` 三处都声明了,但形同虚设,`cargo build --no-default-features --features openai` 仍然会编译全部 14 家。根因三条:(a) 上游 `gproxy-channel` 依赖没有关 `default-features`,所以 `all-channels` 默认还是全进来;(b) `gproxy-engine` 的 `all-channels` 只转发到 `gproxy-channel/all-channels`,没启用自己的 per-channel 子 feature,所以即便代码里有 `#[cfg(feature = "...")]` 也为假;(c) engine 里的 `built_in_model_prices`、`validate_credential_json`、`GproxyEngineBuilder::add_provider_json`、`ProviderStore::add_provider_json`、`bootstrap_credential_on_upsert` 的 match 本来就没写 `#[cfg]` gate。三条在本次一并修掉,`cargo build -p gproxy-sdk --no-default-features --features openai` 现在真的只编译单独那一家 channel。
- **控制台定价编辑器** 收敛为单个三角折叠 —— 嵌套编辑器不再意外级联展开。
- **调度模板描述** 明确说的是上游协议,不是下游客户端 shape。
- **Claude Code OAuth beta 徽章** 去掉误导性的 "always" 后缀,只显示 beta 名。
- **自更新按钮** 和成功 toast 加上中文。
- **`gproxy-engine` crate 文档的 clippy 警告**(`doc_lazy_continuation`)已消除,`cargo clippy -- -D warnings` 不再失败。

#### 移除

- **`gproxy-provider` crate** —— 之前把单渠道访问和多渠道引擎混在一起的聚合 crate。内容分到 `gproxy-channel`(L1)和 `gproxy-engine`(L2)。
- **`gproxy-routing` crate** —— 合并进 `gproxy-engine::routing`(`classify`、`permission`、`rate_limit`、`provider_prefix`、`model_alias`、`model_extraction`、`headers`/原 `sanitize.rs`)。
- **已弃用的 `gproxy_sdk::provider` / `gproxy_sdk::routing` 模块别名** —— 请改用 `gproxy_sdk::channel::*`、`gproxy_sdk::engine::*`、`gproxy_sdk::engine::routing::*`。
- **没人使用的 `ProviderDefinition` 类型** —— 死代码,没有任何消费者。
- **`gproxy-engine::transform_dispatch` 透传文件** —— engine 直接调 `gproxy_protocol::transform::dispatch::*`,那个 14 行 re-export 文件删了。

#### 兼容性

- **二进制 / 服务器运维**:可以从 v1.0.8 直接替换二进制升级,不涉及 DB / HTTP API / admin 客户端 / 配置的任何变更。
- **SDK 库使用者**:breaking change。`gproxy_sdk::provider::*` 和 `gproxy_sdk::routing::*` 路径不复存在。所有 import 必须迁移到 `gproxy_sdk::channel::*`、`gproxy_sdk::engine::*`、`gproxy_sdk::engine::routing::*`(旧的 routing helper),或 `gproxy_sdk::protocol::transform::dispatch::*`(运行时协议分发)。仓库内所有下游消费者都已经迁移完毕。
- **直接依赖 `gproxy-provider` / `gproxy-routing`** 的下游 `Cargo.toml` 必须改成依赖 `gproxy-channel` + `gproxy-engine`,或者依赖 `gproxy-sdk` facade。
- **14 个 channel 的 `Settings` struct** 新增一个由 serde flatten 的 `common: CommonChannelSettings` 字段,旧的 TOML / JSON 配置反序列化完全不变。
- **crates.io 发布**:四个 SDK crate 的元数据和打包都已就绪(已通过 `gproxy-protocol` 的 `cargo publish --dry-run` 和下游三个的 `cargo package --list` 本地验证)。**实际发布还没有发生** —— 本次发版只在本地仓库。真正 publish 时的依赖顺序是 `gproxy-protocol → gproxy-channel → gproxy-engine → gproxy-sdk`,每步之间 sleep ~30 秒等 registry index 更新。

## v1.0.8

> Cross-protocol error bodies finally reach clients in the right shape, OpenAI Responses requests with orphaned tool results stop breaking Claude, and streaming upstream logs record the actual upstream bytes.

### English

#### Fixed

- **Cross-protocol upstream errors reached clients in the wrong shape** — non-2xx upstream error bodies are now translated into the client's declared error schema, with a raw-bytes fallback when the upstream shape doesn't match any declared schema. Client SDKs no longer choke on raw Claude/Gemini JSON.
- **Streaming routes swallowed upstream errors** — upstream errors on cross-protocol streaming routes used to degrade into an empty `[DONE]` stream. Clients now see the real 4xx/5xx error.
- **Orphaned `tool_result` blocks caused Claude 400** — OpenAI Responses API requests using `previous_response_id` with a tool result now synthesize a matching placeholder `tool_use`, so Claude accepts them instead of rejecting the whole request.
- **Streaming upstream logs stored the wrong bytes** — streaming cross-protocol logs now store the real upstream wire bytes, matching the non-streaming path.

#### Changed

- **Streaming passthrough fast path** — routes without transform, raw capture, or alias rewriting are once again forwarded chunk-by-chunk without an extra wrapper layer.

#### Added

- **Per-channel `max_retries_on_429` setting** in every channel's structured editor.
- **TOML download button** on the config export page.

#### Compatibility

- Drop-in upgrade from v1.0.7 — no DB, API, or config changes.
- Streaming upstream-log `response_body` now holds pre-transform upstream bytes instead of post-transform client bytes. Dashboards parsing streaming rows should switch to the upstream protocol's shape.

### 简体中文

#### 修复

- **跨协议的上游错误 shape 不对** — 非 2xx 上游错误体现在会被翻译成客户端声明的错误结构,shape 对不上时回退到原始字节。客户端 SDK 不再因为拿到原始 Claude/Gemini JSON 而解析失败。
- **流式路由吞掉上游错误** — 之前跨协议流式路由遇到上游错误会返回一条空的 `[DONE]` 流,现在客户端能看到真实的 4xx/5xx 错误。
- **孤立 `tool_result` 触发 Claude 400** — OpenAI Responses API 配合 `previous_response_id` 单发 tool 结果时会自动合成匹配的占位 `tool_use`,Claude 不再判整条请求 400。
- **流式上游日志存的字节不对** — 跨协议流式路径现在存的是上游真实字节,与非流式路径一致。

#### 变更

- **流式透传快路径** — 没有 transform、没有抓取、没有别名改写的流式路由重新走 chunk 直通,不再被额外包一层。

#### 新增

- 控制台每个渠道新增 **`max_retries_on_429`** 设置项。
- 配置导出页新增 **TOML 下载按钮**。

#### 兼容性

- 可以从 v1.0.7 直接替换二进制升级,不涉及 DB / API / 配置变更。
- 流式 upstream log 的 `response_body` 现在是上游原始字节,而不是转换后的客户端协议字节。按客户端协议 shape 解析流式行的看板需要改成按上游协议解析。

## v1.0.7

> Self-update unbroken, transform failures actually log the request body, docs site deploys itself.

### English

#### Fixed

- **Self-update failing with `HTTP 302 Found`** — the HTTP client now follows redirects across every build path, so GitHub asset downloads no longer choke on the 302 to the CDN.
- **Pre-upstream transform failures lost the request body in logs** — transform errors thrown before we ever hit a credential now capture the downstream request body, so operators can see which JSON actually failed to parse.

#### Changed

- **HTTP client policy unified** into a single default helper; `update.rs` reuses the engine's HTTP client so self-update inherits the operator's proxy and TLS config.
- **Docker deployment guide rewritten** around the official `ghcr.io/leenhawk/gproxy` image instead of building `Dockerfile.action` locally.

#### Added

- **`GproxyEngine::client()` getter** — public accessor so admin code paths can reuse the engine's configured client.
- **Cloudflare Pages docs deploy** — the release pipeline publishes `https://gproxy.leenhawk.com` automatically on every merge.

#### Compatibility

- Drop-in upgrade from v1.0.6 — no DB, API, or config changes.
- `GproxyEngine::builder().build()` now follows up to 10 redirects (previously zero). SDK consumers that depended on the old behavior must pass their own client explicitly.
- Transform-failure log rows now carry `request_body` instead of `NULL`.

### 简体中文

#### 修复

- **自更新报 `HTTP 302 Found`** — HTTP 客户端现在在所有构建路径上都跟随重定向,GitHub 资源 302 跳 CDN 的场景不再失败。
- **上游前的 transform 失败在日志里丢了请求体** — 在命中凭证之前就抛出的 transform 错误现在会把 downstream 请求体落进上游日志,运维能直接看到是哪段 JSON 解析不动。

#### 变更

- **HTTP 客户端策略** 统一到一个默认 helper;`update.rs` 改为复用 engine 的 HTTP 客户端,自更新流量从此经过运维配置的代理和 TLS 设置。
- **Docker 部署文档** 改为以官方镜像 `ghcr.io/leenhawk/gproxy` 为中心,不再首推本地构建 `Dockerfile.action`。

#### 新增

- **`GproxyEngine::client()` getter** — 对外暴露共享 HTTP 客户端,admin 辅助代码不用再各建一个。
- **Cloudflare Pages 文档部署** — 发版流水线每次合并都会自动更新 `https://gproxy.leenhawk.com`。

#### 兼容性

- 可以从 v1.0.6 直接替换二进制升级,不涉及 DB / API / 配置变更。
- `GproxyEngine::builder().build()` 默认会跟随最多 10 次重定向(之前是 0 次)。依赖旧行为的 SDK 下游需要显式传入自己的 client。
- Transform 失败的日志行现在带 `request_body` 字段,不再是 `NULL`。

## v1.0.6

> Pricing is fully admin-editable end to end, and docs become a proper bilingual Starlight site.

### English

#### Added

- **Admin-editable pricing, end to end** — model prices move out of the compiled-in slice into the DB, and every admin edit is pushed into the running billing engine immediately. Fixes a long-standing bug where edits persisted to the DB but had no effect on billing.
- **Structured pricing editor** in the Models tab, covering all four billing modes (default / flex / scale / priority) in one place, with a JSON view as a fallback.
- **Full `ModelPrice` round-trip through TOML** — priority / flex / scale fields now survive export/import instead of being silently dropped.
- **Bilingual Starlight documentation site** — 25 pages per locale (English + 简体中文) covering the whole gproxy stack, all validated against source. Live at `https://gproxy.leenhawk.com`.
- **Pricing reference page** documenting the `ModelPrice` JSON shape, billing mode selection, and a debugging checklist for when pricing doesn't apply.
- **Batch delete mode** across five admin tables (Users, User Keys, My Keys, Models, Rewrite Rules).

#### Changed

- **Tightened responsive breakpoints** across admin modules so common laptop widths no longer collapse two-column layouts into a single wasteful column.

#### Fixed

- **Usage query button stuck on "querying"** — the summary and rows effects shared a cancellation token and stepped on each other.
- **`x-title` and `http-referer` headers** no longer leak upstream.

#### Removed

- **Legacy `price_each_call` / `price_tiers_json` columns** on `models` — pricing lives in `pricing_json` only.
- **`update_source` TOML field** — self-update is hardcoded to GitHub Releases.
- **Orphan frontend `ModelsModule` route** — admin model management lives entirely inside the provider workspace.

#### Compatibility

- **DB**: the legacy pricing columns are gone. If you're upgrading a DB that still has data in them, migrate it into `pricing_json` before pointing v1.0.6 at it. TOML seed installs are unaffected.
- **Admin clients**: upsert payloads now carry `pricing_json`. Legacy fields stay nullable for schema compatibility but the backend ignores them.
- **Self-update**: deployments can no longer point self-update at a private mirror — use out-of-band updates or patch the download base and rebuild.

### 中文

#### 新增

- **定价后台全可编辑,端到端生效** — 模型价格从编译期嵌入的静态切片搬进 DB,每一次 admin 编辑都会立即推进 billing engine。修复了一个长期存在的 bug:编辑明明写进了 DB,计费引擎却一直读不到。
- **结构化定价编辑器** — 模型 Tab 里覆盖四种计费模式(default / flex / scale / priority),保留 JSON 视图作为 fallback。
- **TOML 导入/导出完整来回 `ModelPrice`** — priority / flex / scale 字段不再在导出时被悄悄丢掉。
- **双语 Starlight 文档站** — 中英文各 25 页,覆盖整个 gproxy 技术栈,全部依据源代码核对。上线在 `https://gproxy.leenhawk.com`。
- **定价参考页**,讲清楚 `ModelPrice` JSON 结构、计费模式选择,以及定价没生效时的排查清单。
- **5 张管理表的批量删除模式** — Users、User Keys、My Keys、Models、Rewrite Rules。

#### 变更

- **后台响应式断点收紧** — 常见笔记本宽度下的双列布局不再塌成一列、空间浪费。

#### 修复

- **用量查询按钮卡在"查询中"** — summary 和 rows 两个 effect 共享的取消 token 被拆开。
- **`x-title` 和 `http-referer` 头** 不再透传到上游。

#### 移除

- **遗留 `price_each_call` / `price_tiers_json` 两列** — 定价只存在于 `pricing_json` 里。
- **`update_source` TOML 字段** — 自更新源硬编码为 GitHub Releases。
- **孤儿前端 `ModelsModule` 路由** — admin 模型管理已全部收敛到 provider 工作区。

#### 兼容性

- **DB**:旧的定价列已移除。若升级的 DB 里仍有数据,请先迁移到 `pricing_json` 再切到 v1.0.6。TOML seed 干净安装不受影响。
- **Admin 客户端**:upsert 请求体现在携带 `pricing_json`。老字段仍然保留为 nullable 以兼容 schema,但后端不再读取。
- **自更新**:部署方不能再把自更新指向私有镜像,请改用带外更新或基于补丁后的下载基址重新编译。

## v1.0.5

> Major refactor: the suffix system is gone, `models` and `model_aliases` are merged, and request-time model resolution is now a single canonical `permission → rewrite → alias → execute → billing` order.

### English

#### Added

- **Model aliases as first-class entries** — aliases now appear in `model_list` / `model_get` responses for OpenAI / Claude / Gemini, and response `"model"` fields are rewritten back to the alias the client sent.
- **Unified `models` table** — `model_aliases` is merged into `models` with an `alias_of` column, so real models and aliases share one admin surface.
- **Pull models from upstream** — new admin endpoint and console button populate the local `models` table from a provider's live model list.
- **Local dispatch for `model_list` / `model_get`** — `*-only` presets default to serving these locally from the `models` table with no upstream round-trip. Non-local dispatch still merges local entries into the upstream response.
- **Alias-level pricing** — admins can override a real model's pricing on a per-alias basis.
- **Provider workspace: dedicated Rewrite Rules tab** — rewrite rules move out of the Config tab's JSON editor into their own two-column list + detail view.
- **Provider workspace: unified Models tab** — real models and aliases live in the same list with filter buttons and an embedded "Pull Models" flow.
- **"+ Add Suffix Variant" dialog** — replaces the deleted Rust suffix system by atomically creating an alias row plus the matching rewrite rules. Covers every preset the old suffix module supported except the four Claude header-modifying suffixes.
- **Rewrite rules editor: typed value input** — the Set action picks between string / number / boolean / null / array / object instead of forcing hand-written JSON.
- **Rewrite rules editor: model-pattern autocomplete** — `model_pattern` input suggests real models and aliases from the current provider.

#### Changed

- **Request pipeline order** — `permission check (original name) → rewrite_rules (original name) → alias resolve → engine.execute → billing`. Permission is checked against the name the client sent, so aliases do not silently inherit their target's permissions.
- **Rewrite rules and billing moved out of the engine** into the handler layer, which is what makes per-alias pricing possible.

#### Fixed

- **`/admin/models/pull` returning HTTP 500** — pull no longer forwards the admin request's headers (including the admin bearer token) to the upstream.
- **Pull-models button was unreachable** — moved into the provider workspace where the sidebar actually links it.

#### Removed

- **Suffix system** — the entire suffix module and all 14 channels' `enable_suffix` flags are gone. The same behavior (`gpt4` vs `gpt4-fast`, etc.) is now expressed as explicit alias rows + rewrite rules.
- **`/admin/model-aliases/*` endpoints and `model_aliases` DB table** — everything runs through `/admin/models/*` now.

#### Compatibility

- **DB**: `alias_of` is a pure column add. The old `model_aliases` table is not dropped automatically — re-enter any aliases you want to keep via the Models tab, or start from a fresh TOML seed.
- **Admin HTTP clients**: clients calling `/admin/model-aliases/*` must migrate to `/admin/models/*` with the new `alias_of` field.
- **Dispatch templates**: `*-only` presets now default `model_list` / `model_get` to Local. Existing providers keep their persisted dispatch; new ones need to pull models before clients can hit those routes.
- **Suffix-style model names** (e.g. `gpt-4o-fast`, `claude-3-opus-thinking-high`) no longer work out of the box. Re-express them as explicit alias rows with per-channel rewrite rules.

### 中文

#### 新增

- **模型别名作为一等条目** — 别名现在会出现在 OpenAI / Claude / Gemini 的 `model_list` / `model_get` 响应中,响应的 `"model"` 字段也会被改写回客户端发送的别名。
- **统一的 `models` 表** — `model_aliases` 合并进 `models`,新增 `alias_of` 列,真实模型和别名共享同一套管理入口。
- **从上游拉取模型** — 新的 admin 接口和控制台按钮,从 provider 的实时模型列表填充本地 `models` 表。
- **`model_list` / `model_get` 的 Local 调度** — `*-only` 预设默认本地服务,不再透传上游。非 Local 调度仍会把本地条目合并进上游响应。
- **按别名定价** — 管理员可以在别名行上单独覆写真实模型的价格。
- **Provider 工作区:独立的"参数改写规则" Tab** — rewrite_rules 从 Config Tab 的 JSON 编辑器里搬出,独立成两栏的列表 + 详情界面。
- **Provider 工作区:统一的 Models Tab** — 真实模型和别名同在一个列表,带过滤按钮和内嵌的拉取模型流程。
- **"+ 添加后缀变体" 对话框** — 替代已删除的 Rust suffix 系统,原子地创建别名行 + 对应 rewrite_rules。覆盖旧 suffix 模块的所有预设,但不包括 Claude 那 4 个改 header 的后缀。
- **改写规则编辑器:类型化值输入** — Set 动作从手写 JSON 改为按类型选择(string / number / boolean / null / array / object)。
- **改写规则编辑器:模型名自动补全** — `model_pattern` 输入框会提示当前 provider 下的真实模型和别名。

#### 变更

- **请求管线顺序** — `权限检查(原始名)→ rewrite_rules(原始名)→ 别名解析 → engine.execute → 计费`。权限按客户端发送的名字检查,别名不会默默继承其指向模型的权限。
- **Rewrite rules 和计费移出 engine**,改由 handler 执行,这也是按别名定价能真正生效的前提。

#### 修复

- **`/admin/models/pull` 返回 500** — pull 不再把 admin 请求头(含 admin bearer token)透传给上游。
- **拉取模型按钮不可达** — 按钮挪到 provider 工作区,侧边栏能链接到的位置。

#### 移除

- **Suffix 系统** — 整个 suffix 模块和 14 个 channel 上的 `enable_suffix` 开关全部删除。同样的效果(`gpt4` 和 `gpt4-fast` 等)现在用显式的别名行 + rewrite_rules 表达。
- **`/admin/model-aliases/*` 端点和 `model_aliases` 表** — 全部增删改查走 `/admin/models/*`。

#### 兼容性

- **DB**:`alias_of` 是一次纯加列变更。旧的 `model_aliases` 表不会被自动删除,想保留的别名请升级后从 Models Tab 重新录入,或者用新的 TOML seed 干净安装。
- **Admin HTTP 客户端**:调用 `/admin/model-aliases/*` 的客户端必须迁移到 `/admin/models/*`,并带上新的 `alias_of` 字段。
- **调度模板**:`*-only` 预设把 `model_list` / `model_get` 默认改为 Local。已有 provider 保留原调度;新建 provider 在客户端命中之前需要先拉取模型。
- **Suffix 风格的模型名**(如 `gpt-4o-fast`、`claude-3-opus-thinking-high`)开箱即用的支持没了,请改写成显式的别名行 + 渠道级 rewrite_rules。

## v1.0.4

### English

#### Added

- **Channel-level rewrite rules** — a new `rewrite_rules` field on all 14 channel Settings structs rewrites the request body before it's finalized. Rules support JSON path targeting with glob matching, and the console ships a dedicated editor with full i18n.
- **Dispatch template presets for custom channel** — built-in dispatch template presets when configuring custom channels, and dispatch templates are now visible for all channel types, not just custom.

#### Fixed

- **Request log query button stuck on loading** — no longer gets permanently stuck.
- **HTTP client protocol negotiation** — removed the `http1_only` restriction and enabled proper HTTP/1.1 support, improving compatibility with HTTP/1.1-only proxies.
- **Sampling parameter stripping** — anthropic/claudecode channels now strip unsupported sampling parameters based on the target model.
- **Dispatch template passthrough** — `*-only` templates correctly use passthrough+transform for `model_list` / `model_get`.
- **Session-expired toast** no longer flashes before the page reload.
- **Update-available toast color** changed from error-red to green success style.
- **Noisy ORM logging** — `sqlx` and `sea_orm` now default to `warn`.
- **Dispatch / sanitize rules overflow** — both panels scroll when content exceeds the viewport.
- **Upstream proxy placeholder** — the input field now shows a placeholder hint.
- **Frontend i18n** — `alias`, `enable_suffix`, `enable_magic_cache` labels translated; "模型" renamed to "模型价格表" / "Model Pricing"; `sanitize_rules` renamed to "消息重写规则" / "Message Rewrite Rules".

### 中文

#### 新增

- **渠道级重写规则** — 全部 14 个渠道 Settings 新增 `rewrite_rules` 字段,支持在请求最终发送前按路径重写请求体,规则支持 JSON path 定位与 glob 匹配。控制台提供专用结构化编辑器,完整支持中英文。
- **Custom 渠道调度模板预设** — 控制台配置 custom 渠道时提供内置调度模板预设,且调度模板现在对所有渠道类型可见。

#### 修复

- **请求日志查询按钮卡死** — 查询按钮不再永久停留在 loading 状态。
- **HTTP 客户端协议协商** — 移除 `http1_only` 限制并启用 HTTP/1.1 支持,改善仅 HTTP/1.1 代理的兼容性。
- **采样参数裁剪** — anthropic/claudecode 渠道按目标模型裁剪不支持的采样参数。
- **调度模板透传** — `*-only` 模板正确使用 passthrough+transform 处理 `model_list` / `model_get`。
- **会话过期 toast** 页面刷新前不再闪现过期提示。
- **更新可用 toast 颜色** 从红色错误样式改为绿色成功样式。
- **ORM 日志降噪** — `sqlx` 和 `sea_orm` 日志级别默认设为 `warn`。
- **调度规则 / 重写规则溢出** — 两个面板内容超出视口时改为滚动。
- **上游代理占位提示** — 上游代理输入框现在显示占位符提示。
- **前端国际化** — `alias`、`enable_suffix`、`enable_magic_cache` 标签已正确翻译;"模型"改名为"模型价格表" / "Model Pricing";`sanitize_rules` 改名为"消息重写规则" / "Message Rewrite Rules"。

## v1.0.3

### English

#### Added

- **Suffix system for model-list / model-get** — suffix modifiers (e.g. `-thinking-high`, `-fast`) are expanded in model list responses and rewritten in model get responses, so clients can discover available suffix variants.
- **Suffix per-channel toggle** — new `enable_suffix` setting enables/disables suffix processing per channel.
- **VertexExpress local model catalogue** — model list/get is served from a static catalogue embedded at compile time, since Vertex AI Express has no standard model-listing endpoint.
- **Vertex SA token bootstrap on credential upsert** — Vertex credentials with `client_email` + `private_key` now auto-fetch an access token on admin upsert so the first request has valid auth.

#### Fixed

- **GeminiCLI / Antigravity model list** — both channels now correctly route model list/get through their respective quota/model endpoints and normalize responses to standard Gemini format.
- **Vertex model list normalization** — Vertex's `publisherModels` responses are now converted to standard Gemini `models` format.
- **Vertex / VertexExpress header filtering** — `anthropic-version` and `anthropic-beta` are dropped before forwarding to Google.
- **Vertex GeminiCLI-style User-Agent** — Vertex requests now send the `User-Agent` and `x-goog-api-client` headers matching Gemini CLI traffic.
- **Engine HTTP client proxy** — DB proxy settings now take effect after bootstrap; the engine client used to be built before DB config loaded.
- **Engine HTTP/1.1 for standard client** — non-spoof wreq client uses `http1_only()` for reliable proxy traversal.
- **HTTP client request dispatch** — switched to `client.request().send()` so proxy/TLS settings propagate correctly.
- **Frontend: VertexExpress credential** field renamed from `access_token` to `api_key`.
- **Frontend: Vertex credential** — added missing optional fields (`private_key_id`, `client_id`, `token_uri`).

### 中文

#### 新增

- **Suffix 系统支持 model-list / model-get** — suffix 修饰符(如 `-thinking-high`、`-fast`)会在模型列表响应中展开、在模型详情响应中回写,客户端可以发现可用的 suffix 变体。
- **Suffix 按渠道开关** — 新增 `enable_suffix` 配置项,可按渠道启用/禁用 suffix 处理。
- **VertexExpress 本地模型目录** — model list/get 请求从编译时嵌入的静态模型目录返回,因为 Vertex AI Express 没有标准的模型列表端点。
- **Vertex SA 凭证 upsert 自动换 token** — 通过 admin API 添加包含 `client_email` 和 `private_key` 的 Vertex 凭证时,自动获取 access token,首次请求不会因空 token 失败。

#### 修复

- **GeminiCLI / Antigravity 模型列表** — 两个渠道现在正确通过各自的配额/模型端点路由 model list/get 请求,并将响应整形为标准 Gemini 格式。
- **Vertex 模型列表整形** — Vertex AI 返回的 `publisherModels`(含完整资源路径)现在被转换为标准 Gemini `models` 格式。
- **Vertex / VertexExpress 头过滤** — 转发到 Google 端点前丢弃 `anthropic-version` 和 `anthropic-beta` 头。
- **Vertex GeminiCLI 风格 User-Agent** — Vertex 请求现在发送匹配 Gemini CLI 流量的 `User-Agent` 和 `x-goog-api-client` 头。
- **Engine HTTP 客户端代理** — 数据库代理设置现在在自举后生效;之前 engine 客户端在 DB 配置加载前就已构建。
- **Engine 标准客户端 HTTP/1.1** — 非伪装 wreq 客户端使用 `http1_only()` 确保代理穿透可靠。
- **HTTP 客户端请求调度** — 改为 `client.request().send()`,确保代理/TLS 设置正确传递。
- **前端:VertexExpress 凭证** 字段从 `access_token` 改为 `api_key`。
- **前端:Vertex 凭证** — 添加缺失的可选字段(`private_key_id`、`client_id`、`token_uri`)。

## v1.0.2

### English

#### Added

- **WebSocket per-model usage tracking** — when the client switches models mid-session (e.g. via `response.create`), usage is segmented per model and recorded separately instead of attributing all tokens to the last model.
- **WebSocket upstream message logging** — WS session end now records an upstream request log containing all client→server and server→client messages as request/response body.

### 中文

#### 新增

- **WebSocket 按模型分段用量** — 客户端在 WS 会话中切换模型时,用量按模型分段记录,不再把所有 token 归到最后一个模型。
- **WebSocket 上游消息日志** — WS session 结束时记录上游请求日志,包含所有客户端→服务器和服务器→客户端消息。

## v1.0.1

### English

#### Added

- **Upstream request logging** — quota queries and cookie exchange HTTP steps are now recorded in the `upstream_requests` table, giving full visibility into every outbound call the proxy makes.
- **Streaming body capture** — both downstream and upstream logs defer recording until the stream ends, so `response_body` is populated for streaming requests. Controlled by `enable_downstream_log_body` / `enable_upstream_log_body`.
- **Auto-check for updates** — the console fires a background version check after admin login and shows a toast when a new release is available.
- **Wildcard model permission for admins** — creating or promoting a user to admin now automatically seeds a `*` model permission.
- **Credential import via raw JSON** — the console credential form offers a single JSON textarea for direct paste import; plain cookie or API-key strings are auto-wrapped into the correct JSON shape.

#### Fixed

- **Credential token refresh persisted** — refreshed `access_token` values are now written back to the database and updated in memory, so they survive restarts.
- **Cookie-only credentials** — credentials with only a `cookie` field (no `access_token`) can now be deserialized; bootstrap populates the token.
- **Claude Code org info backfill** — `billing_type`, `rate_limit_tier`, `account_uuid`, and `user_email` are now extracted from the bootstrap /organizations response when the token endpoint omits them.
- **Version check endpoint** — the updater now uses the GitHub Releases API instead of a nonexistent `latest.json` URL.
- **Console session stability** — 401 responses from upstream provider routes no longer clear the admin session; only `/admin/*` and `/login` 401s trigger logout.
- **Request log loading loop** — removed `pageCursors` from the row-loading effect dependency array to break an infinite re-render cascade.
- **Cache breakpoint TTL aliases** — `"5m"` and `"1h"` are now accepted as serde aliases alongside `"ttl5m"` / `"ttl1h"`.
- **Credential quota reset time** — displayed in local timezone via `toLocaleString()` instead of raw ISO strings.
- **Credential card layout** — title, badge, and action buttons now wrap cleanly.
- **Android CI** — updated `setup-android` action to v4.

#### Changed

- **`subscription_type` removed** — `subscription_type` / `billing_type` / `organization_type` fields dropped from credential, cookie exchange, OAuth profile, and frontend forms. Only `rate_limit_tier` is retained.
- **Cache breakpoint simplification** — `content_position` / `content_index` removed from breakpoint rules; breakpoints always use flat block positioning across all messages.
- **i18n** — shortened Chinese cache breakpoint position labels (正数 / 倒数).

### 中文

#### 新增

- **上游请求日志** — 配额查询和 cookie 交换的每一步 HTTP 请求现在都会记录到 `upstream_requests` 表,完整追踪代理发出的所有出站调用。
- **流式响应 body 采集** — 下游和上游日志均推迟到流结束后再写入,流式请求的 `response_body` 不再为空。由 `enable_downstream_log_body` / `enable_upstream_log_body` 配置控制。
- **自动检查更新** — 管理员登录后控制台会在后台检查新版本,有新版时弹出提示。
- **管理员自动授权通配符模型权限** — 新建或提升为 admin 的用户会自动获得 `*` 模型权限,无需手动配置即可调用所有 provider。
- **凭证 JSON 粘贴导入** — 控制台凭证表单新增单个 JSON 文本框,支持直接粘贴完整 JSON;也可粘贴纯 cookie 或 API key 字符串,自动包装为正确格式。

#### 修复

- **凭证 token 刷新落库** — 通过 refresh_token 刷新的 access_token 现在会同时更新内存和写入数据库,重启后不丢失。
- **纯 cookie 凭证** — 仅含 `cookie` 字段(无 `access_token`)的凭证现在可以正常反序列化,bootstrap 流程会自动补全 token。
- **Claude Code 组织信息回填** — 当 token 端点未返回组织信息时,`billing_type`、`rate_limit_tier`、`account_uuid`、`user_email` 会从 bootstrap /organizations 响应中提取并回填。
- **版本检查端点** — 更新检查改用 GitHub Releases API,不再请求不存在的 `latest.json`。
- **控制台会话稳定性** — 上游 provider 路由返回的 401 不再误触发管理员登出,仅 `/admin/*` 和 `/login` 路径的 401 才清除会话。
- **请求日志加载死循环** — 从行加载 effect 的依赖数组中移除 `pageCursors`,打破无限重渲染循环。
- **缓存断点 TTL 别名** — `"5m"` 和 `"1h"` 现在可以作为 serde 别名使用,与 `"ttl5m"` / `"ttl1h"` 等效。
- **凭证配额重置时间** — 使用 `toLocaleString()` 显示本地时区,不再显示原始 ISO 字符串。
- **凭证卡片布局** — 标题、标记和操作按钮正确换行。
- **Android CI** — `setup-android` action 升级到 v4。

#### 变更

- **移除 `subscription_type`** — 从凭证、cookie 交换、OAuth profile 和前端表单中删除 `subscription_type` / `billing_type` / `organization_type` 字段,仅保留 `rate_limit_tier`。
- **缓存断点简化** — 移除 breakpoint 规则中的 `content_position` / `content_index`,断点统一使用跨所有消息的扁平 block 定位。
- **国际化** — 缩短中文缓存断点位置标签(正数 / 倒数)。

## v1.0.0

> **Breaking release.** gproxy v1.0.0 is a full ground-up rewrite of the v0.3.x line. Treat it as a brand-new project: workspace layout, storage schema, HTTP API, admin surface, TOML config format, CLI flags, and provider settings have all changed and are **not** compatible with v0.3.42 or earlier. There is no in-place upgrade path.

### English

#### Added

- **Brand-new three-layer workspace** — `sdk/` owns protocol conversion, provider execution, credential health, and routing; `crates/` owns HTTP routing, admin API, storage, and `AppState`; `apps/` holds the main server and a standalone recorder binary.
- **New storage layer** built on SeaORM + SQLx with first-class support for SQLite, PostgreSQL, and MySQL. Schema auto-syncs on startup.
- **New embedded browser console** mounted at `/console`, shipped inside the binary.
- **New admin HTTP API** under `/admin/*` covering providers, credentials, models, aliases, users, keys, permissions, rate limits, quotas, logs, and self-update.
- **New user HTTP API** under `/user/*` for self-service key management, quota lookup, and usage queries.
- **New provider proxy surface** with both scoped (`/{provider}/v1/...`) and unscoped (`/v1/...`) routes covering Claude Messages, OpenAI Chat Completions, OpenAI Responses, Embeddings, Images, Models, Gemini v1beta, and provider file APIs.
- **New WebSocket bridging** — passthrough, OpenAI ↔ Gemini Live, and Gemini Live ↔ OpenAI Responses.
- **Security hardening** — Argon2id password hashing, SHA-256 API key digests with constant-time comparison, optional XChaCha20Poly1305 field-level encryption for credentials, and admin-response masking for credential secrets.
- **Optional Redis backend** via the `redis` Cargo feature for multi-instance rate limiting, quota reservation, and cache affinity.
- **New TOML seed config format** driving first-time bootstrap.
- **Standalone `gproxy-recorder` binary** for capturing upstream LLM traffic independently of the main server.
- **Graceful shutdown pipeline** — bounded worker drain, final usage flush, and health-broadcaster flush.

#### Changed

- Workspace version bumped from `0.3.42` to **`1.0.0`**.
- All provider execution now goes through `gproxy-sdk`'s `GproxyEngine`. Provider registration, credential dispatch, protocol conversion, and cache affinity are owned by the SDK.
- **DB-first admin mutations** — write storage → sync `AppState` → rebuild `GproxyEngine` atomically via `ArcSwap`. Hot reload via `POST /admin/reload`.
- **Memory-first read paths** — auth, permission checks, rate limiting, quota checks, and alias resolution all run out of in-memory snapshots. The DB is no longer on the request hot path.
- **Bootstrap precedence** — existing DB → TOML seed → built-in defaults.
- **CLI / environment variables reworked** around the new app.
- **Credential health** now managed by the SDK at runtime and snapshotted to a dedicated table.

#### Removed

- The entire v0.3.x admin UI, provider settings schema, and channel-specific toggles. Legacy fields like `claudecode_enable_billing_header`, `enable_claude_1m_sonnet`, `priority_tier`, etc. are not carried over.
- Legacy v0.3.x storage tables and on-disk layout. No automated migration.
- Old `gproxy-admin` and `gproxy-middleware` crates — their responsibilities are split across `gproxy-api`, `gproxy-server`, and the `sdk/` crates.
- Per-channel credential status semantics — the new SDK classifies failures uniformly across providers.

#### Compatibility

- **Hard break from v0.3.x.** No automated migration path. Stand up a fresh database, regenerate admin and user credentials, and re-enter providers / models / aliases / permissions / quotas against the new v1 schema.
- Old `gproxy.toml` files from v0.3.x won't load as-is. Rewrite them against `gproxy.example.toml` / `gproxy.example.full.toml` first.
- HTTP clients that called v0.3.x admin routes must be updated to the new `/admin/*` surface.
- User-facing provider proxy routes are compatible at the protocol level with standard Claude / OpenAI / Gemini clients, but auth, model aliasing, and permission errors use the v1 error shape.
- Credential secrets, passwords, and API keys should be re-imported after `DATABASE_SECRET_KEY` has been decided. Switching it later is not supported in-place.
- Multi-instance deployments that relied on in-process counters must now opt into the `redis` feature and point `GPROXY_REDIS_URL` at a shared Redis instance.

### 中文

#### 新增

- **全新三层 workspace 布局** — `sdk/` 负责协议转换、provider 执行、凭证健康与路由;`crates/` 负责 HTTP 路由、admin API、存储与 `AppState`;`apps/` 存放主服务和独立的录制工具。
- **全新存储层**,基于 SeaORM + SQLx,原生支持 SQLite、PostgreSQL、MySQL。启动时自动同步 schema。
- **全新嵌入式浏览器控制台**,挂载在 `/console`,通过 rust-embed 打入二进制。
- **全新 admin API**:`/admin/*` 下统一提供 providers、credentials、models、aliases、users、keys、权限、限流、配额、日志与自更新接口。
- **全新 user API**:`/user/*`,供用户自助管理 API key、查询配额与用量。
- **全新的 provider 代理入口**,同时提供 scoped(`/{provider}/v1/...`)与 unscoped(`/v1/...`)两种路径,覆盖 Claude Messages、OpenAI Chat Completions、OpenAI Responses、Embeddings、Images、Models、Gemini v1beta,以及 provider 文件 API。
- **全新的 WebSocket 桥接** — 同协议透传、OpenAI ↔ Gemini Live、Gemini Live ↔ OpenAI Responses。
- **安全加固** — Argon2id 密码哈希、SHA-256 API key 摘要 + 常量时间比对、可选的 XChaCha20Poly1305 字段级加密、admin API 响应中的凭证脱敏。
- **可选的 Redis 后端**:`redis` Cargo feature,用于多实例环境下的限流、配额预留和缓存亲和。
- **全新的 TOML 种子配置格式**,用于首次启动时初始化 DB。
- **独立的 `gproxy-recorder` 二进制**,脱离主服务独立抓取上游 LLM 流量。
- **优雅关闭流水线** — worker 收敛、用量终态刷写、健康广播 flush。

#### 变更

- workspace 版本由 `0.3.42` 升级到 **`1.0.0`**。
- 所有 provider 执行现在都通过 `gproxy-sdk` 的 `GproxyEngine`。provider 注册、凭证调度、协议转换与缓存亲和由 SDK 掌握。
- **DB-first 管理变更**:先写存储 → 同步 `AppState` → 通过 `ArcSwap` 原子替换 `GproxyEngine`。热重载通过 `POST /admin/reload` 暴露。
- **Memory-first 读路径**:鉴权、权限、限流、配额检查、别名解析等全部走内存快照,数据库不再出现在请求热路径上。
- **Bootstrap 优先级**:已有 DB → TOML 种子 → 内置默认。
- **CLI / 环境变量** 围绕新应用重新梳理。
- **凭证健康状态** 现在由 SDK 在运行时维护,并快照到专门的表里。

#### 移除

- 整套 v0.3.x 的后台 UI、provider 设置结构与渠道专用开关。`claudecode_enable_billing_header`、`enable_claude_1m_sonnet`、`priority_tier` 等字段均未保留。
- v0.3.x 的存储表结构与落盘布局。不提供自动迁移。
- 旧的 `gproxy-admin`、`gproxy-middleware` crate,其职责已拆分到 `gproxy-api`、`gproxy-server` 及 `sdk/` 下。
- 按渠道定制的凭证健康语义 — 新 SDK 跨 provider 统一分类失败。

#### 兼容性

- **这是相对 v0.3.x 的硬断代。** 不提供任何自动迁移路径。请按全新项目对待:新建数据库,重新生成 admin / user 凭证,并在 v1 schema 下重新配置 providers / models / aliases / permissions / quotas。
- v0.3.x 的 `gproxy.toml` 无法直接加载。请参照 `gproxy.example.toml` / `gproxy.example.full.toml` 重新编写后再启动 v1。
- 依赖 v0.3.x admin 路由的 HTTP 客户端必须全面迁移到新的 `/admin/*` 接口。
- 面向用户的 provider 代理路由在协议层仍兼容标准 Claude / OpenAI / Gemini 客户端;但鉴权、模型别名、权限等错误会按 v1 错误结构返回。
- 凭证密钥、用户密码、API key 应在确定 `DATABASE_SECRET_KEY` 之后再重新导入。运行后再切换 `DATABASE_SECRET_KEY` 不是受支持的原地操作。
- 依赖 v0.3.x 进程内限流 / 配额计数的多实例部署,必须启用 `redis` feature 并把 `GPROXY_REDIS_URL` 指向共享 Redis。
