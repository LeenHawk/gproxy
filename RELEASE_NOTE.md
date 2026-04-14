# Release Notes

## v1.0.8

> **Cross-protocol error bodies finally make it to the client in the
> right schema, orphaned `tool_result` messages stop breaking Claude
> requests, and streaming upstream logs now store the actual wire
> bytes.** The headline fix: when a Claude/Gemini/OpenAI upstream
> returns a non-2xx error body, the engine now converts it to the
> client's declared error shape (e.g. Claude `{"type":"error",...}` →
> OpenAI `{"error":{...}}`) instead of handing the raw JSON to an SDK
> that can't parse it — with a raw-bytes fallback when the upstream
> shape doesn't match any declared schema. Streaming error responses
> finally reach the client too, after a buffer-and-convert fast path
> replaces the broken SSE transformer that used to swallow the error
> body and emit only `[DONE]`. On the transform side, a new
> `push_message_block` utility centralizes Claude message-building
> invariants across every `*→Claude` converter, fixing an OpenAI
> Responses-API bug where `previous_response_id` + fresh
> `function_call_output` produced orphaned `tool_result` blocks and
> Claude returned a 400. The console picks up a per-channel
> `max_retries_on_429` field and a one-click TOML download on the
> config export page.

### English

#### Fixed

- **Non-2xx upstream errors reached clients in the wrong protocol
  schema** — each provider uses a different error shape (Claude
  `{"type":"error","error":{...}}`, OpenAI `{"error":{...}}`, Gemini
  `{"error":{"code":N,...}}`), and before this release the engine only
  ran `transform_response` on 2xx bodies. An OpenAI-speaking client
  that hit a Claude 400 got the raw Claude JSON back, which the SDK
  couldn't parse, so dashboards saw a generic "invalid response" on
  what was really a simple upstream 400 (e.g. `prompt is too long`).
  `sdk/gproxy-provider/src/engine.rs` and
  `sdk/gproxy-provider/src/transform_dispatch.rs` now route error
  bodies through the new `convert_error_body_or_raw` helper, which
  tries the declared error variant via `BodyEnvelope::from_body_bytes`
  and falls back to raw upstream bytes on schema mismatch (e.g. codex
  returning `{"detail":{"code":"deactivated_workspace"}}`, which isn't
  any declared error schema). Claude-error-to-OpenAI-error conversion
  is covered by a new integration test.
- **Streaming endpoints swallowed upstream error bodies** — on a
  cross-protocol transform route (e.g. client speaks
  OpenAI-chat-completions, upstream is Claude), a non-2xx upstream
  response was fed to the inline per-chunk SSE transformer, which
  couldn't parse the JSON error body as an SSE frame, yielded nothing,
  and emitted only a synthetic `[DONE]`. The client saw an empty
  success stream instead of the actual 4xx/5xx error. `execute_stream`
  now detects `!is_success` upstream early, buffers the full error
  body (which is always a small complete JSON, not a real SSE
  stream), runs it through `convert_error_body_or_raw`, and returns
  a single-chunk `ExecuteBody::Stream` with the converted bytes. The
  raw pre-conversion upstream bytes are still captured for the
  upstream log so operators can see what actually came over the wire.
- **Orphaned `tool_result` blocks caused Claude 400 on OpenAI
  Responses-API requests** — Claude's API requires "each `tool_result`
  block must have a corresponding `tool_use` block in the previous
  message," but the OpenAI Responses API lets clients send only
  `function_call_output` items when using `previous_response_id`
  (the `tool_use` side lives in the prior turn, which the client is
  referencing by id instead of resending). The legacy `*→Claude`
  transforms built messages by blindly pushing blocks, so these
  requests ended up with a leading `user`/`tool_result` message and
  no matching `assistant`/`tool_use` — Claude returned 400 every
  time. The new `push_message_block` helper (see Added) synthesizes
  a placeholder `tool_use` block with the matching `id` whenever it
  detects an orphaned `tool_result`, so the request now satisfies
  Claude's pairing rule and goes through.
- **Adjacent same-role messages from multi-block transforms** — the
  per-transform `push_block_message` helpers produced two separate
  `user` messages for two consecutive `tool_result` pushes (and
  similarly for assistant blocks), which Claude's API rejects as
  malformed. `push_message_block` now merges consecutive blocks for
  the same role into a single `BetaMessageContent::Blocks` message,
  so every `*→Claude` transform produces a well-formed message list
  by construction.
- **Streaming upstream logs stored post-transform bytes instead of
  pre-transform wire bytes** — the handler's old
  `accumulated_body: Vec<u8>` collected chunks as they were *yielded
  downstream*, so for cross-protocol routes the `response_body`
  column in `upstream_requests` held the converted (OpenAI/Gemini/…)
  bytes, not what Claude/OpenAI-upstream actually sent. This diverged
  from the non-stream path, which stores the pre-transform bytes via
  `raw_response_body_for_log`. A new stream wrapper
  (`wrap_upstream_response_stream`) now tees upstream bytes into an
  `Arc<Mutex<Vec<u8>>>` capture buffer *before* they reach the
  transformer, and the handler reads it after the stream drains.
  Stream and non-stream paths are now byte-for-byte consistent in
  the upstream log.

#### Changed

- **Passthrough streaming fast path** — when a stream route has no
  transformer, no `raw_capture`, and no `response_model_override`, the
  engine now hands `response.body` through to the client unwrapped
  instead of going through a per-chunk `try_stream!` loop. This
  reclaims the passthrough latency that was lost when `accumulated_body`
  was added. The wrapper is only spliced in when at least one of raw
  capture, transform, or alias rewriting is active.
- **`rand 0.9.4` / `rand_core 0.10.1`** — minor dependency bumps.
  Picks up upstream API cleanups; no gproxy code changes required.

#### Added

- **`convert_error_body_or_raw(src_op, src_proto, dst_op, dst_proto,
  body)`** in `sdk/gproxy-provider/src/transform_dispatch.rs` —
  converts an upstream non-2xx body from the upstream protocol's
  error schema to the client's expected error schema via
  `transform_response`, substituting `GenerateContent` for streaming
  ops (error bodies share the non-stream schema). Passthrough routes
  (same src/dst protocol and op) skip conversion entirely. On schema
  mismatch the helper logs at debug level with the full
  `src_op`/`src_proto`/`dst_op`/`dst_proto` context and returns the
  raw bytes so no error information is lost. Three unit tests cover
  Claude→OpenAI rewriting, codex-shape fallback, and the passthrough
  case.
- **`ExecuteResult.stream_raw_capture: Option<Arc<Mutex<Vec<u8>>>>`**
  — new field on the SDK result type, populated by
  `execute_stream` when `enable_upstream_log &&
  enable_upstream_log_body` and the route actually sees a raw-capture
  tee. The handler reads the buffer after the stream drains and
  copies it into `meta.response_body`, so
  `upstream_requests.response_body` contains the pre-transform wire
  bytes that correspond to what the non-stream path already stored.
  `None` on passthrough-with-logging-off routes and on the error-body
  fast path's re-use (which seeds the buffer with pre-conversion
  bytes itself).
- **`wrap_upstream_response_stream`** in
  `sdk/gproxy-provider/src/engine.rs` — single stream-combinator that
  applies, in order: raw-byte tee into `raw_capture`, optional
  per-chunk `StreamResponseTransformer`, and optional model-alias
  rewriting. Replaces the previous two inlined `try_stream!` loops
  (one for transform + alias, one for alias-only) with a unified
  helper whose behaviour is covered by two unit tests
  (`wrap_response_stream_tees_raw_bytes_in_passthrough_mode`,
  `wrap_response_stream_pure_passthrough_yields_chunks_unchanged`).
- **`push_message_block(messages, role, block)`** in
  `sdk/gproxy-protocol/src/transform/claude/utils.rs` — central
  utility for building Claude `messages` lists from any non-Claude
  source. Maintains two invariants:
  1. Consecutive blocks for the same role are merged into one
     `BetaMessageContent::Blocks` message (no adjacent same-role
     messages).
  2. Whenever a `tool_result` block is appended to a `user` message,
     the immediately-preceding assistant message is checked for a
     matching `tool_use` block; if none exists, a placeholder
     `tool_use` (named `tool_use_placeholder`) is synthesized in the
     assistant slot — either by promoting an existing assistant
     message's content into blocks and appending, or by inserting a
     new assistant message before the trailing user one.
  Exported from `transform::claude::utils` and re-exported from
  `transform::utils` so non-Claude callers don't need a cross-module
  dependency. Every `*→Claude` request transform (`gemini`,
  `openai_chat_completions`, `openai_response`, `openai_compact`,
  `openai_count_tokens`) is migrated to call it instead of pushing
  messages directly. Covered by 9 unit tests, including the exact
  orphaned-tool_result shape reported in production.
- **Per-channel `max_retries_on_429` setting in ConfigTab** — every
  channel's structured editor now exposes an optional integer input
  bound to the backend's per-credential 429-without-`retry-after`
  retry cap (backend default: 3). Empty input is omitted from the
  settings JSON so the backend default still applies. i18n strings
  added in both locales (`field.max_retries_on_429`).
- **TOML download button on the config export page** — `ConfigExport`
  module grows a neutral `Download` button alongside the existing
  `Export`. Clicking it ships the current export as
  `gproxy-config-<ISO-timestamp>.toml` via a `Blob` + `<a>`-click. If
  the user hasn't clicked `Export` yet, `Download` fetches the TOML
  first and then triggers the file save. New i18n key:
  `common.download`.

#### Compatibility

- **No DB, API, or config changes.** `settings.toml`,
  `global_settings`, and the admin API schema are all untouched.
  This is a drop-in upgrade from v1.0.7 — just swap the binary.
- **Upstream-request log `response_body` semantics change for
  streaming routes.** Previously, streaming cross-protocol routes
  stored the *post-transform* bytes (what the client saw). They now
  store the *pre-transform upstream wire bytes* (what the upstream
  actually sent), matching the non-stream path. Dashboards that
  were parsing `response_body` as the client-protocol shape on
  streaming rows need to switch to parsing it as the upstream
  protocol's shape. Operators who were relying on this to debug
  what the upstream sent benefit without doing anything.
- **Upstream-error log rows now include `response_body` for cross-
  protocol routes.** Previously the body was often empty on error
  rows because the SSE transformer dropped it. Dashboards that
  filtered error rows by `response_body = ''` will see fewer matches.
- **`ExecuteResult` gains `stream_raw_capture`.** SDK consumers that
  pattern-match `ExecuteResult { .. }` or construct it by name must
  add the new field. Existing users of `ExecuteResult::body` / `meta`
  / `usage` / etc. are unaffected.
- **Orphaned `tool_result` requests now succeed where they used to
  400.** Callers that were filtering their traffic *because* Claude
  rejected these will see traffic resume. The placeholder
  `tool_use` blocks synthesised by `push_message_block` are named
  `tool_use_placeholder` with an empty `input` object; downstream
  log analysis that wants to distinguish "real upstream tool_use"
  from "placeholder injected by gproxy" can filter on that name.

### 简体中文

#### 修复

- **非 2xx 上游错误体抵达客户端时协议不对** —
  各家 provider 的错误体 shape 都不一样（Claude
  `{"type":"error","error":{...}}`，OpenAI `{"error":{...}}`，Gemini
  `{"error":{"code":N,...}}`），而 v1.0.7 之前 engine 只对 2xx body 走
  `transform_response`。一个 OpenAI-chat-completions 的客户端打到
  Claude 上游，遇到 400 会拿到原始 Claude JSON，SDK 完全解析不了，
  日志里看到的就是一个笼统的「invalid response」，但实际上只是
  `prompt is too long` 这种上游 400。`sdk/gproxy-provider/src/engine.rs`
  和 `sdk/gproxy-provider/src/transform_dispatch.rs` 现在把错误体
  路由到新加的 `convert_error_body_or_raw` helper：它先尝试通过
  `BodyEnvelope::from_body_bytes` 走声明的 error 变体，如果 shape 对
  不上（比如 codex 回的 `{"detail":{"code":"deactivated_workspace"}}`
  不匹配任何声明的 error 模式），就回退到原始上游字节，保证
  错误信息不会丢。Claude-error→OpenAI-error 的转换有集成测试覆盖。
- **流式端点把上游错误体吞掉了** — 跨协议 transform 路由
  （例如客户端说 OpenAI-chat-completions、上游是 Claude）上，
  非 2xx 上游响应会被送进 per-chunk SSE transformer，而它根本
  没法把 JSON 错误体当作 SSE 帧解析，于是产不出任何输出，
  最后只合成一个 `[DONE]`，客户端看到的是一条空的成功流，
  而不是真实的 4xx/5xx 错误。`execute_stream` 现在会提前检测
  `!is_success` 的上游响应，把整个错误体缓冲起来（错误体永远是
  一小段完整 JSON，不是真正的 SSE 流），跑一遍
  `convert_error_body_or_raw`，再以单 chunk 的形式返回一个
  `ExecuteBody::Stream`。转换前的原始上游字节仍然会被抓给
  upstream log，让运维能看到线上实际传过来什么。
- **孤立的 `tool_result` 块让 OpenAI Responses API 请求被 Claude 打回
  400** — Claude 的 API 要求「每个 `tool_result` 块都必须在上一条
  消息里有对应的 `tool_use` 块」，但 OpenAI Responses API 允许
  客户端在使用 `previous_response_id` 时只发 `function_call_output`
  条目（对应的 `tool_use` 是在上一轮，用 id 引用而不是重发）。老的
  `*→Claude` transform 只是一味地 push block，结果就出现一条开头
  就是 `user`/`tool_result`、却没有匹配的 `assistant`/`tool_use` 的
  消息 —— Claude 每次都直接 400。新加的 `push_message_block`
  helper（见「新增」）在检测到孤立 `tool_result` 时，会合成一个
  带匹配 `id` 的占位 `tool_use` block，请求从此能满足 Claude 的
  配对规则顺利通过。
- **多块 transform 产生相邻的同角色消息** — 之前各 transform 的
  `push_block_message` helper 在连续两次 push `tool_result` 时会
  产生两条独立的 `user` 消息（assistant 块同理），Claude 的 API
  会把这种结构判为非法。`push_message_block` 会自动把同角色的
  连续块合并进同一条 `BetaMessageContent::Blocks` 消息，从结构
  上保证每个 `*→Claude` transform 产出的消息列表是合法的。
- **流式 upstream log 存的是 transform 后的字节，不是上游真实字节** —
  handler 以前的 `accumulated_body: Vec<u8>` 是在 *chunk 往下游发出去
  时* 拼起来的，所以跨协议路由上 `upstream_requests.response_body`
  存的其实是转换后（OpenAI/Gemini/…）的字节，而不是 Claude 或
  OpenAI 上游真正发的内容。这和非流式路径通过
  `raw_response_body_for_log` 存的 pre-transform 字节不一致。
  新的 stream wrapper（`wrap_upstream_response_stream`）会在
  transformer *碰到 chunk 之前*，先把上游字节 tee 进一个
  `Arc<Mutex<Vec<u8>>>` 的抓取缓冲里，handler 在流结束后把它读出来。
  从此流式和非流式路径在 upstream log 上逐字节一致。

#### 变更

- **流式 passthrough 快路径** — 当一个流路由既没有 transformer，
  又没有 `raw_capture`，也没有 `response_model_override` 时，engine
  现在直接把 `response.body` 原样透给客户端，而不再穿过一个
  per-chunk 的 `try_stream!` 循环。这把当初为了加
  `accumulated_body` 而损失的 passthrough 延迟找了回来。wrapper
  只在抓取、转换、别名改写这三件事至少有一件开启时才会被接进来。
- **`rand 0.9.4` / `rand_core 0.10.1`** —
  次要的依赖升级，吃掉上游 API 清理，gproxy 侧没有代码改动。

#### 新增

- **`convert_error_body_or_raw(src_op, src_proto, dst_op, dst_proto,
  body)`** —— 在 `sdk/gproxy-provider/src/transform_dispatch.rs` 里，
  把上游非 2xx body 从上游协议的 error 模式转换到客户端声明的 error
  模式，流式 op 会被替换成对应的 `GenerateContent`（错误体和非流式
  共享同一套 schema）。passthrough 路由（src 和 dst 的 protocol + op
  全相同）直接跳过转换。shape 对不上时会在 debug 级别打出完整的
  `src_op` / `src_proto` / `dst_op` / `dst_proto` 上下文并返回原始
  字节，保证错误信息不丢。三条单测覆盖 Claude→OpenAI 改写、
  codex-shape 回退和 passthrough 三种场景。
- **`ExecuteResult.stream_raw_capture: Option<Arc<Mutex<Vec<u8>>>>`** ——
  SDK 结果类型新增字段，只在 `enable_upstream_log &&
  enable_upstream_log_body` 并且路由确实走了 raw-capture tee 的时候
  由 `execute_stream` 填充。handler 在流结束后读这个 buffer 并
  塞进 `meta.response_body`，让 `upstream_requests.response_body` 存的
  是和非流式路径一致的 pre-transform 字节。不开 upstream log 的
  passthrough 路由以及错误体快路径（它自己给 buffer 种好了转换前
  字节）上这个字段都是 `None`。
- **`wrap_upstream_response_stream`** ——
  `sdk/gproxy-provider/src/engine.rs` 里新的单入口 stream combinator，
  按顺序执行：原始字节 tee 进 `raw_capture`、可选的
  per-chunk `StreamResponseTransformer`、可选的模型别名改写。
  替代之前两处内联的 `try_stream!` 循环（一处负责 transform+alias、
  一处只负责 alias），行为有两条单测覆盖
  （`wrap_response_stream_tees_raw_bytes_in_passthrough_mode`、
  `wrap_response_stream_pure_passthrough_yields_chunks_unchanged`）。
- **`push_message_block(messages, role, block)`** ——
  `sdk/gproxy-protocol/src/transform/claude/utils.rs` 里新加的
  Claude messages 构建中枢。从任何非 Claude 源构建消息时都应该走它。
  它维护两条不变量：
  1. 同角色的连续 block 合并进同一条
     `BetaMessageContent::Blocks`，不允许出现相邻的同角色消息。
  2. 每次往 `user` 消息追加 `tool_result` block 时，检查紧挨着的
     前一条 assistant 消息里有没有匹配的 `tool_use`；如果没有，就
     在 assistant 槽位上合成一个占位 `tool_use`（名字叫
     `tool_use_placeholder`）—— 要么把已有 assistant 消息的内容
     提升为 blocks 再 append，要么在尾部 user 消息之前插入一条
     新的 assistant 消息。
  从 `transform::claude::utils` 导出，同时在 `transform::utils` 里
  re-export，非 Claude 的 caller 不需要跨模块依赖 `claude` 子模块。
  每一个 `*→Claude` 的 request transform（`gemini`、
  `openai_chat_completions`、`openai_response`、`openai_compact`、
  `openai_count_tokens`）都已经改为调用它，而不是直接 push 消息。
  配了 9 条单测，包括线上报过的那条孤立 `tool_result` 的精确
  还原用例。
- **ConfigTab 每 channel 的 `max_retries_on_429` 设置** —
  每个 channel 的结构化编辑器都多了一个可选整数输入，绑定到
  后端的「每凭证 429-without-`retry-after` 重试上限」（后端默认 3）。
  留空时不会写进 settings JSON，让后端默认值生效。
  两种语言都加了 i18n key（`field.max_retries_on_429`）。
- **配置导出页的 TOML 下载按钮** — `ConfigExport` 模块在原本的
  `Export` 按钮旁边多了一个 neutral 风格的 `Download` 按钮。
  点击会把当前导出内容通过 `Blob` + `<a>`-click 保存成
  `gproxy-config-<ISO-timestamp>.toml`。如果用户还没点过 `Export`，
  `Download` 会先去拉 TOML 再触发文件保存。新 i18n key：
  `common.download`。

#### 兼容性

- **不涉及 DB、API、配置变更。** `settings.toml`、
  `global_settings`、admin API schema 全部原封不动，v1.0.7 可以
  直接替换二进制升级到 v1.0.8。
- **流式路由的 `response_body` 语义变了。** 之前流式跨协议路由
  存的是 *transform 之后* 的字节（客户端看到的那份），现在存的是
  *transform 之前* 的上游原始字节（上游实际发的那份），和非流式
  路径一致。之前按客户端协议 shape 解析流式行 `response_body`
  的看板需要改成按上游协议解析。靠这份字段排查上游实际回了
  什么的运维什么都不用改，直接获益。
- **跨协议路由的 upstream 错误日志行现在会带 `response_body`。**
  之前这些行经常是空的，因为 SSE transformer 把 body 吞掉了。
  按 `response_body = ''` 筛错误行的看板会看到匹配减少。
- **`ExecuteResult` 新增 `stream_raw_capture` 字段。** SDK 下游
  如果用 `ExecuteResult { .. }` 模式匹配或按名构造这个结构体，
  需要把新字段加上。只读 `body` / `meta` / `usage` 之类的消费者
  不受影响。
- **孤立 `tool_result` 请求从此能通。** 之前因为 Claude 把这种
  请求判 400 而在上游侧屏蔽流量的 caller，会看到这部分流量恢复。
  `push_message_block` 合成的占位 `tool_use` 块名字统一是
  `tool_use_placeholder`，`input` 是空对象；需要区分「上游真实
  tool_use」和「gproxy 注入的占位」的日志分析，可以按这个名字过滤。

## v1.0.7

> **Self-update is unbroken, failing transforms finally tell you which
> request broke them, and the docs site deploys itself.** The headline
> fix centralizes wreq client policy in the engine so every HTTP path
> (including self-update) follows redirects — GitHub asset downloads
> stop failing on their 302 to the CDN. Pre-upstream transform errors
> now capture the original downstream request body in the upstream
> log, so operators actually see *which* JSON failed to parse. The
> release pipeline grows a Cloudflare Pages deploy job for the docs
> site, and the Docker deployment page is rewritten around the
> official `ghcr.io/leenhawk/gproxy` image.

### English

#### Fixed

- **Self-update failing with `download failed: HTTP 302 Found`** —
  GitHub serves every `/releases/download/...` asset as a 302 to the
  CDN host, but `wreq`'s default redirect policy is
  `redirect::Policy::none()`, so `wreq::get(url)` in
  `crates/gproxy-api/src/admin/update.rs` returned the redirect
  response verbatim and `download_bytes` / `download_text` rejected
  it at the `status().is_success()` check. The update path never
  touched the engine client either, so the fresh per-call default
  client inherited none of the runtime configuration.
- **Pre-upstream transform failures lost the request body in logs**
  — when `transform_dispatch::transform_request` failed before we
  ever sent anything upstream (e.g. a malformed `tools[]` entry
  failing to deserialize into `ResponseTool`), the error bubbled up
  as `ExecuteError { meta: None, .. }` and
  `record_execute_error_logs` wrote an upstream-log row with
  `request_body = NULL`, leaving operators a 500 with no way to see
  which JSON actually failed. `GproxyEngine::execute` and
  `execute_stream` now catch the transform error, clone the
  original downstream body beforehand, and synthesize an
  `UpstreamRequestMeta` via the new `build_transform_error` helper
  so the offending body lands in the log. URL / headers /
  response fields stay empty because the request never hit the
  wire; `enable_upstream_log` / `enable_upstream_log_body` are still
  honored.

#### Changed

- **Single source of truth for HTTP client policy** — new
  `default_http_client()` helper in
  `sdk/gproxy-provider/src/engine.rs` centralizes the global wreq
  client policy (`redirect::Policy::limited(10)`). Every build path
  now routes through it:
  - `GproxyEngineBuilder::build()` uses it as the default fallback
    (was `self.client.unwrap_or_default()`), so bare
    `GproxyEngine::builder().build()` — used by tests and several
    admin-only bootstrap paths — no longer produces a client that
    drops redirects.
  - `configure_clients` and `with_new_clients` set `.redirect(...)`
    on both the normal and spoof-emulation builders, and their
    `Err` fallbacks route through `default_http_client()` instead
    of `wreq::Client::default()`.
  This also closes a latent footgun: if `configure_clients` ever
  failed to build (bad proxy URL, TLS init error), the process used
  to silently fall back to a fully-unconfigured default client.
  The fallback now at least keeps the redirect policy.
- **`update.rs` reuses the engine's HTTP client** — `check_update`
  and `perform_update` grab `state.engine().client().clone()` and
  pass it through to `fetch_github_manifest`, `download_bytes`, and
  `download_text`. The three helpers no longer call `wreq::get(url)`
  / `wreq::Client::new()` at all. Practical upshot: self-update
  traffic now inherits the operator's configured upstream proxy,
  TLS settings, and whatever else the engine is built with —
  previously it silently bypassed all of them.
- **Docker deployment guide rewritten around the official image**
  — `docs/src/content/docs/deployment/docker.md` (and the Chinese
  mirror) now leads with `docker pull ghcr.io/leenhawk/gproxy:latest`
  instead of "build `Dockerfile.action` locally," and documents the
  full tag matrix (`latest` / `vX.Y.Z` / `staging` × glibc / musl,
  plus per-arch suffixes). The installation pages cross-reference
  the new guidance so new users don't start by building an image
  they don't need to.

#### Added

- **`GproxyEngine::client()` getter** — public accessor exposing
  the shared `&wreq::Client`, so auxiliary admin code paths can
  reuse the engine's configured client instead of constructing
  their own. The spoof client stays private; the normal client is
  the right choice for anything that is not upstream provider
  traffic.
- **`build_transform_error` helper** in
  `sdk/gproxy-provider/src/engine.rs` — synthesizes an
  `UpstreamRequestMeta` for the pre-upstream transform failure path
  so operators get the downstream request body in the upstream log
  even when we never reached a credential or a URL.
- **Cloudflare Pages docs deploy job** — the
  `.github/workflows/release-binary.yml` pipeline gains a
  `deploy-docs-cloudflare` job that runs on default-branch pushes
  and on releases: pnpm-installs, builds `docs/`, then ships the
  result to Cloudflare Pages via `cloudflare/wrangler-action@v3`
  using the `cloudflare` environment's
  `CLOUDFLARE_API_TOKEN` / `CLOUDFLARE_ACCOUNT_ID` /
  `CLOUDFLARE_PROJECT_ID` secrets. The docs site at
  `https://gproxy.leenhawk.com` now updates automatically with every
  merge.
- **`sea-orm-migration` workspace dependency** — declared in
  `[workspace.dependencies]` in preparation for an upcoming
  managed-migration pass. No crate pulls it in yet, so this has no
  runtime effect in v1.0.7.

#### Compatibility

- **No DB, API, or config changes.** `settings.toml`,
  `global_settings`, and the admin API schema are all untouched.
  This is a drop-in upgrade from v1.0.6 — just swap the binary.
- **Engine builder defaults shift.** `GproxyEngine::builder().build()`
  now yields a client that follows up to 10 redirects, where v1.0.6
  and earlier yielded a client that followed zero. SDK consumers
  that were relying on the old behavior (e.g. intentionally
  capturing 3xx responses as terminal) must explicitly pass their
  own `wreq::Client` via `http_client(...)` /
  `configure_clients(...)`.
- **Transform-failure log rows now include `request_body`** where
  they previously had `NULL`. `url` / `request_headers` /
  `response_*` on those rows are still empty strings / empty
  arrays / NULL — the request never hit the wire, so there's
  nothing real to record. Dashboards that were filtering transform
  failures by `url = ''` will still work; ones that were filtering
  by `request_body IS NULL` will need to check for the actual error
  message instead.

### 简体中文

#### 修复

- **自更新报 `download failed: HTTP 302 Found`** — GitHub
  的 `/releases/download/...` 资源永远是 302 到 CDN 域名的，
  但 `wreq` 的默认重定向策略是 `redirect::Policy::none()`，所以
  `crates/gproxy-api/src/admin/update.rs` 里 `wreq::get(url)`
  拿到的是 302 本身，`download_bytes` / `download_text` 在
  `status().is_success()` 这一步就直接拒绝。更新路径根本没
  接触到 engine 的 client，所以每次新建的默认 client 也继承不到
  任何运行时配置。
- **上游前的 transform 失败在日志里丢了 request body** —
  当 `transform_dispatch::transform_request` 在真正发请求之前
  就失败（例如 `tools[]` 里有一个字段无法反序列化成
  `ResponseTool`），错误会以 `ExecuteError { meta: None, .. }`
  冒上来，`record_execute_error_logs` 写出的 upstream log 行
  `request_body = NULL`，运维只能看到一个 500 但看不到到底是
  哪段 JSON 解析不动。`GproxyEngine::execute` 和 `execute_stream`
  现在会捕获这个 transform 错误，提前克隆原始 downstream body，
  再通过新加的 `build_transform_error` helper 合成一个
  `UpstreamRequestMeta`，让出问题的 body 能落进日志。URL /
  headers / response 相关字段留空，因为请求根本没发上游；
  `enable_upstream_log` / `enable_upstream_log_body` 仍然生效。

#### 变更

- **HTTP client 策略统一到一个入口** —
  `sdk/gproxy-provider/src/engine.rs` 新增 `default_http_client()`
  helper，把全局 wreq client 策略（`redirect::Policy::limited(10)`）
  收敛到一个地方。所有构建路径现在都走它：
  - `GproxyEngineBuilder::build()` 的默认兜底从
    `self.client.unwrap_or_default()` 改成
    `unwrap_or_else(default_http_client)`，裸的
    `GproxyEngine::builder().build()` —— 测试和若干 admin-only
    bootstrap 路径都在用 —— 不会再构造出一个不跟随重定向的 client。
  - `configure_clients` 和 `with_new_clients` 给普通 client 和
    spoof client 的 builder 都加了 `.redirect(...)`，而且它们的
    `Err` 兜底分支也从 `wreq::Client::default()` 切到
    `default_http_client()`。
  顺带堵了一个潜在陷阱：如果 `configure_clients` 构建失败（代理
  URL 有问题、TLS 初始化失败之类），之前会静默退回到一个完全
  未配置的默认 client。现在至少兜底 client 仍然会跟随重定向。
- **`update.rs` 改为复用 engine 的 HTTP client** —
  `check_update` 和 `perform_update` 取
  `state.engine().client().clone()` 传给 `fetch_github_manifest`、
  `download_bytes` 和 `download_text`，三个 helper 都不再调用
  `wreq::get(url)` / `wreq::Client::new()`。实际效果：自更新流量
  现在会经过运维配置的上游代理、TLS 设置以及 engine 上的其它
  配置 —— 此前是悄悄绕过了所有这些配置。
- **Docker 部署文档改为以官方镜像为中心** —
  `docs/src/content/docs/deployment/docker.md`（以及中文镜像）
  现在首推 `docker pull ghcr.io/leenhawk/gproxy:latest`，而不是
  「本地构建 `Dockerfile.action`」，并补齐了完整的 tag 矩阵
  （`latest` / `vX.Y.Z` / `staging` × glibc / musl，以及各自的
  per-arch 后缀）。安装页也相应调整，避免新用户上来就去构建
  一个他们根本不需要构建的镜像。

#### 新增

- **`GproxyEngine::client()` getter** —
  公开访问器，暴露共享的 `&wreq::Client`，方便 admin 辅助
  代码路径复用 engine 已配置好的 client，而不是各自再建一个。
  spoof client 仍然保持私有；非上游 provider 流量应该用这个
  普通 client。
- **`build_transform_error` helper** —
  `sdk/gproxy-provider/src/engine.rs` 新增，专门给上游前的
  transform 失败路径合成 `UpstreamRequestMeta`，让运维在根本
  还没选到 credential、没拿到 URL 的时候，也能在 upstream log 里
  看到 downstream 原始 body。
- **Cloudflare Pages 文档部署 Job** —
  `.github/workflows/release-binary.yml` 新增 `deploy-docs-cloudflare`
  job：在默认分支推送和 release 事件上触发，pnpm install
  → 构建 `docs/` → 通过 `cloudflare/wrangler-action@v3` 推到
  Cloudflare Pages，使用 `cloudflare` environment 下的
  `CLOUDFLARE_API_TOKEN` / `CLOUDFLARE_ACCOUNT_ID` /
  `CLOUDFLARE_PROJECT_ID` 三个 secret。从此
  `https://gproxy.leenhawk.com` 每次合并都会自动更新。
- **`sea-orm-migration` workspace 依赖** —
  在 `[workspace.dependencies]` 中声明，为后续引入受管迁移做
  铺垫。v1.0.7 里还没有 crate 实际引用它，运行时没有任何
  影响。

#### 兼容性

- **不涉及 DB、API、配置变更。** `settings.toml`、
  `global_settings` 和 admin API schema 全部原封不动，v1.0.6
  可以直接替换二进制升级到 v1.0.7。
- **Engine builder 默认行为变了。** `GproxyEngine::builder().build()`
  现在会构造一个跟随最多 10 次重定向的 client，v1.0.6 及更早版本
  是不跟随。依赖旧行为（比如故意把 3xx 当终止响应抓取）的 SDK
  下游使用者，需要通过 `http_client(...)` /
  `configure_clients(...)` 显式传入自己的 `wreq::Client`。
- **Transform 失败的日志行现在会带 `request_body`**，而之前
  是 `NULL`。这些行的 `url` / `request_headers` / `response_*`
  仍然是空字符串 / 空数组 / NULL —— 请求根本没发上游，没有
  真实内容可记。按 `url = ''` 筛 transform 失败的 dashboard
  仍然可用；按 `request_body IS NULL` 筛的则需要改成按实际
  错误信息判断。

## v1.0.6

> **Pricing is now fully admin-editable, end to end.** Model prices move
> out of the compiled-in `&'static [ModelPrice]` slice into a
> `pricing_json` column on the `models` table, the provider store holds
> an `ArcSwap<Vec<ModelPrice>>` that bootstrap and every admin mutation
> push into, and the console grows a structured editor that covers all
> four billing modes. The docs site is rewritten as a full bilingual
> Starlight site (25 pages × 2 locales) including a new pricing
> reference page.

### English

#### Added

- **`models.pricing_json` column** — nullable `TEXT` column on the
  `models` entity holding the full `ModelPrice` JSON blob: all four
  billing modes (`default` / `flex` / `scale` / `priority`) in one
  place. Threaded through `ModelQueryRow`,
  `ModelWrite`, `store_query/admin`, and `write_sink`. `MemoryModel` now
  carries a single `Option<ModelPrice>` deserialized from the column on
  load and re-serialized on admin upsert, so the complete pricing shape
  round-trips through the DB.
- **Hot-swappable provider pricing** — `ProviderInstance.model_pricing`
  goes from `&'static [ModelPrice]` to
  `ArcSwap<Vec<ModelPrice>>`, and the `ProviderRuntime` trait gains
  `set_model_pricing`. `Engine::set_model_pricing(provider, prices)` is
  exposed for host wiring. `AppState::push_pricing_to_engine` rebuilds
  a `ModelPrice` slice from the current `MemoryModel` snapshot and
  pushes it into the engine; it runs once during bootstrap after
  `replace_models` and again from every admin mutation handler that
  changes the model set. **This fixes a long-standing bug** where admin
  edits to `price_each_call` / `price_tiers_json` were persisted to the
  DB but the billing engine kept reading the compiled-in slice forever.
- **Structured pricing editor in `ModelsTab`** — the lone
  `pricing_json` textarea is replaced with a `PricingEditor` component
  that toggles between "Structured" and "JSON" views. Structured view
  provides: a single `price_each_call` USD input; an add/remove
  `price_tiers` table with 7 per-tier fields (`input_tokens_up_to`
  plus the six per-token unit prices); and collapsible `<details>`
  sections for `flex` / `scale` / `priority`, each with its own
  `price_each_call` and tiers table and auto-expanded when the model
  already has pricing in that mode. All numeric fields are held as
  strings in form state so users can type freely.
- **TOML import/export round-trips full `ModelPrice`** — `ModelToml`
  gains six new fields (`flex_price_each_call` / `flex_price_tiers`,
  `scale_price_each_call` / `scale_price_tiers`,
  `priority_price_each_call` / `priority_price_tiers`). All nine
  pricing fields use `#[serde(default, skip_serializing_if = ...)]` so
  minimal models still produce compact TOML. Previously the shape only
  carried default-mode tiers, so admin-edited priority pricing was
  silently dropped on export.
- **Bilingual Starlight documentation site** — the placeholder docs
  template is replaced with a comprehensive site covering the whole
  gproxy stack. 25 pages per locale (English + 简体中文), all validated
  against the source rather than inferred from READMEs. Sections:
  Introduction, Getting Started (installation, quick start, first
  request for both aggregated `/v1` and scoped `/{provider}/v1`
  routing), Guides (providers & channels, models & aliases, users &
  API keys, permissions / rate limits / quotas, rewrite rules, Claude
  prompt caching, adding a channel, embedded console, observability),
  Reference (env vars, TOML config, dispatch table, database backends,
  graceful shutdown, Rust SDK), and Deployment (release build, Docker).
  Root READMEs rewritten as project overviews pointing at the docs
  site.
- **Pricing reference page** — new
  `reference/pricing.md` in both English and Chinese covers the
  `ModelPrice` JSON shape, the per-1M-token formula, billing mode
  selection, exact-then-default price matching, and debugging checklist
  for when a price doesn't apply. Linked from `guides/models.md` and
  from the Starlight sidebar.
- **Unit tests for the new pricing and usage paths** — an
  `unknown-provider` branch assertion on `set_model_pricing`.
- **Batch delete mode across 5 admin tables** — the Users, User Keys,
  My Keys, Models, and Rewrite Rules lists gain a reusable "batch"
  toggle. Activating it swaps per-row delete buttons for checkboxes and
  surfaces a `[Select all] [Clear] [Delete N] [Exit]` action bar.
  Confirmation goes through `window.confirm`, matching existing delete
  UX. Four of the five tables reuse existing `*/batch-delete` handlers
  already exposed by `crates/gproxy-api/src/admin/mod.rs`; the fifth
  (`/user/keys/batch-delete`) is new — user-scoped with an up-front
  ownership check against `keys_for_user` to prevent cross-user key
  deletion. Rewrite rules batch delete is purely client-side (filters
  the in-memory `rewrite_rules` JSON) since that resource has no
  backend CRUD. Implementation is factored into two shared primitives
  in `frontend/console/src/components/`: a generic `useBatchSelection`
  hook (selection state, stale-key pruning on row refetch, confirm +
  delete orchestration) and a presentational `BatchActionBar`.

#### Changed

- **`ModelsTab` model-pricing field** — replaced `price_each_call` +
  `price_tiers_json` text inputs with the new structured
  `PricingEditor` / JSON textarea toggle. `MemoryModelRow` and
  `ModelWrite` TS types now expose `pricing_json` instead of the two
  legacy fields; the legacy fields remain on `ModelWrite` as nullable
  for API-schema compatibility but are always written as `null` by the
  console. `i18n` strings `common.priceEachCall` /
  `common.priceTiersJson` removed.
- **Atomic admin upsert validation** — `batch_upsert_models` now
  pre-validates every item's `pricing_json` before writing any of
  them, so a malformed entry halfway through a batch no longer leaves
  half of the DB updated.
- **`push_pricing_to_engine` is best-effort / last-writer-wins** —
  documented as such so future readers don't reach for a mutex. Logs
  a `warn!` when `set_model_pricing` returns `false` (i.e. the
  provider is missing from the engine store), so the no-op state
  surfaces instead of being silent.
- **Responsive breakpoints tightened across admin modules** — most
  admin pages used `xl:grid-cols` (1280px) for sidebar+content splits
  and `lg:grid-cols-2` (1024px) for forms, so common laptop widths
  collapsed to a single wasteful column. Drop those to `lg:` / `md:`
  so the intended two-column layouts appear at 1024px / 768px; add
  `sm:` fallback to 6-field filter grids; let 8-metric rows shrink to
  1 column on small phones; scope the mobile full-width `.btn` rule to
  `.toolbar-shell` so inline table/card buttons stay compact; cap
  toast `min-width` to the viewport; and give the suffix-dialog modal
  padding so it no longer hugs the screen edge on phones.

#### Fixed

- **UsageModule query button stuck on "querying"** — `UsageModule`
  (admin) and `MyUsageModule` (user) shared a single `queryTokenRef`
  between their summary and rows effects. When `setActiveQuery` fired
  both effects, the rows effect bumped the counter before the summary
  request resolved, so the summary's `.finally()` check
  (`queryTokenRef.current === token`) failed and `setLoadingMeta(false)`
  was never called — pinning the button on "querying" forever. Split
  into `summaryTokenRef` + `rowsTokenRef` so the cancellation tokens
  are independent, matching the pattern in `useRequestsModuleState`.
- **`x-title` and `http-referer` headers leaked upstream** — added to
  the request-header denylist in both
  `gproxy-server/src/middleware/sanitize.rs` and
  `sdk/gproxy-routing/src/sanitize.rs`, so OpenRouter-style client
  metadata stops reaching upstream channels that might reject or log
  it.

#### Removed

- **Legacy `price_each_call` + `price_tiers_json` columns on `models`**
  — the two columns are removed from the SeaORM entity,
  `ModelQueryRow`, `ModelWrite`, `store_query/admin`, `write_sink`, and
  `write/event`. Pricing lives in `pricing_json` only. The 2.3→2.4
  transition intentionally left the legacy columns on disk temporarily
  to allow a backfill; this release retires them.
- **Update source configuration** — `update_source` TOML field,
  related i18n messages, admin types, and the
  `.github/workflows/release-binary.yml` internal update server flow
  are removed. The standalone `DownloadsPage.astro` is gone; docs
  download links now point at GitHub Releases.
- **Orphan frontend `ModelsModule`** — the module was wired into
  `app/modules.tsx`'s `activeModule` switch as `case "models"`, but
  `buildAdminNavItems` never emitted a nav item for `"models"`, so it
  was unreachable. Admin model management already lives inside the
  provider workspace's Models tab.
- **`PriceTier` from `gproxy-core`** — downstream consumers use
  `gproxy_sdk::provider::billing::ModelPriceTier` instead.

#### Compatibility

- **DB schema**: `models.pricing_json` is a pure column add, picked up
  by the SeaORM schema-sync step on startup. Existing rows get `NULL`
  and fall back to whatever `ModelPrice` the provider compiled in. The
  legacy `price_each_call` and `price_tiers_json` columns are
  **removed** from the entity — if you are upgrading a DB that still
  has data in those columns, migrate them into `pricing_json` **before**
  pointing v1.0.6 at the DB. A clean install via TOML seed is not
  affected.
- **Admin clients**: upsert payloads now carry `pricing_json: string |
  null`. Legacy `price_each_call` / `price_tiers_json` fields remain
  on the admin API as nullable for schema compatibility, but the
  backend no longer reads them — clients should stop sending them and
  send `pricing_json` instead.
- **TOML exports**: pricing blocks now include the extra flex / scale
  / priority fields when set. Existing TOML files without those fields
  continue to import cleanly.
- **Self-update source is now hardcoded to GitHub Releases** — the
  `update_source` configuration is gone, so deployments can no
  longer point the in-process self-updater at a private mirror or
  reverse proxy. The in-place upgrade flow itself still works and
  pulls from `LeenHawk/gproxy` on GitHub; anyone who was relying on
  a custom mirror must either update that binary out-of-band and
  restart, or rebuild gproxy with a patched download base.

### 中文

#### 新增

- **`models.pricing_json` 列** — 在 `models` 实体上新增可空的 `TEXT`
  列，存放完整的 `ModelPrice` JSON：四种计费模式（`default` / `flex`
  / `scale` / `priority`）全部放在一个字段
  里。变更贯穿 `ModelQueryRow` / `ModelWrite` / `store_query/admin`
  / `write_sink`。`MemoryModel` 改为携带一个 `Option<ModelPrice>`，
  在加载时从新列反序列化、在 admin upsert 时重新序列化，使得完整的
  pricing 结构能在 DB 中完整来回。
- **可热替换的 provider 定价** — `ProviderInstance.model_pricing`
  从 `&'static [ModelPrice]` 切换到
  `ArcSwap<Vec<ModelPrice>>`，`ProviderRuntime` trait 新增
  `set_model_pricing`。`Engine::set_model_pricing(provider, prices)`
  作为 host 接入点对外暴露。`AppState::push_pricing_to_engine` 会用
  当前的 `MemoryModel` 快照重建一份 `ModelPrice` 并推送到 engine —
  启动完成 `replace_models` 之后推一次，之后每一次会改动模型集的
  admin handler 都会再推一次。**这修复了一个长期存在的 bug**：
  admin 对 `price_each_call` / `price_tiers_json` 的编辑明明写进
  了 DB，billing engine 却一直在读编译期嵌入的 `&'static` 切片。
- **`ModelsTab` 的结构化定价编辑器** — 把原先孤零零的
  `pricing_json` textarea 替换成 `PricingEditor` 组件，提供
  "结构化" 与 "JSON" 两种视图切换。结构化视图包含：单个
  `price_each_call` USD 输入框；可增删的 `price_tiers` 表格（每条
  7 个字段 —— `input_tokens_up_to` 加六个 per-token 单价）；以及
  `flex` / `scale` / `priority` 三个可折叠 `<details>` 段落，各自
  维护独立的 `price_each_call` 与 tiers 表格，对应模式已有定价时
  自动展开。所有数值字段在表单状态里以字符串存储，允许用户自由
  输入。
- **TOML 导入 / 导出完整来回 `ModelPrice`** — `ModelToml` 新增
  6 个字段（`flex_price_each_call` / `flex_price_tiers`、
  `scale_price_each_call` / `scale_price_tiers`、
  `priority_price_each_call` / `priority_price_tiers`）。全部 9 个
  定价字段都使用 `#[serde(default, skip_serializing_if = ...)]`，
  最小化的 model 仍然生成紧凑的 TOML。此前结构只承载 default 模式
  的 tiers，所以 admin 编辑的 priority 定价在 TOML 导出时会被悄悄
  丢掉。
- **双语 Starlight 文档站** — 占位的 docs 模板替换为覆盖整个 gproxy
  技术栈的完整站点。每个语言 25 页（English + 简体中文），全部依据
  源代码核对、不是从 README 里推断。章节包括：Introduction、
  Getting Started（installation / quick start / first request，
  聚合 `/v1` 与 scoped `/{provider}/v1` 两种路由模式都覆盖）、
  Guides（providers & channels、models & aliases、users & API
  keys、permissions / rate limits / quotas、rewrite rules、
  Claude prompt caching、adding a channel、embedded console、
  observability）、Reference（env vars、TOML config、dispatch
  table、database backends、graceful shutdown、Rust SDK）、
  Deployment（release build、Docker）。根 README 重写为项目总览，
  链接回 docs 站。
- **定价参考页** — 新增
  `reference/pricing.md`（中英双语），涵盖 `ModelPrice` JSON
  结构、per-1M-token 公式、计费模式选择、精确匹配→默认 fallback，
  以及当定价没生效时的排查清单。从 `guides/models.md` 和 Starlight
  侧边栏均有入口。
- **针对新定价路径的单测** — `set_model_pricing` 对未知 provider
  的 false 断言。
- **5 张管理表的批量删除模式** — Users、User Keys、My Keys、Models、
  Rewrite Rules 共享同一套「批量」开关。开启后逐行删除按钮变成复选
  框，顶部出现 `[全选] [清空] [删除 N 项] [退出]` 操作条。确认走
  `window.confirm`，与既有删除交互一致。其中 4 张表复用
  `crates/gproxy-api/src/admin/mod.rs` 已有的 `*/batch-delete`
  handler；第 5 个 `/user/keys/batch-delete` 是新增的，用户态，
  在删除前用 `keys_for_user` 做一次所有权校验，防止越权删除他人
  密钥。Rewrite Rules 因为没有后端 CRUD，批量删除纯客户端（过滤
  `rewrite_rules` JSON 数组后整体 re-save）。前端抽出两个共享
  原语到 `frontend/console/src/components/`：泛型
  `useBatchSelection` hook（选中状态、rows 变化时自动剔除陈旧
  key、confirm + 删除编排）和展示型组件 `BatchActionBar`。

#### 变更

- **`ModelsTab` 的定价字段** — `price_each_call` + `price_tiers_json`
  两个文本输入框被替换为新的 `PricingEditor` / JSON textarea 切换
  组件。`MemoryModelRow` 与 `ModelWrite` 的 TS 类型改为暴露
  `pricing_json` 而不是旧的两个字段；旧字段在 `ModelWrite` 上仍
  保留为 nullable 以兼容 API schema，但前端始终写 `null`。i18n
  的 `common.priceEachCall` / `common.priceTiersJson` 已删除。
- **Admin upsert 原子预校验** — `batch_upsert_models` 现在在写入
  任何一项之前，先把整个 batch 里每一项的 `pricing_json` 全部校验
  一遍，避免中途出现格式错误的条目把一半数据写进 DB、另一半没
  写的情况。
- **`push_pricing_to_engine` 是 best-effort / last-writer-wins**
  语义 — 代码注释里显式标注，免得后来人想去上锁。当
  `set_model_pricing` 返回 `false`（即 provider 不在 engine store
  里）时打 `warn!`，让"没推进去"的状态浮出水面而不是静默。
- **管理后台响应式断点收紧** — 大部分 admin 页面在侧边栏+内容
  布局上用 `xl:grid-cols`（1280px），表单里用
  `lg:grid-cols-2`（1024px），导致常见笔记本宽度塌成一列、空间
  浪费严重。把这些下调到 `lg:` / `md:`，让双列布局在 1024px /
  768px 就能生效；6 字段过滤器网格增加 `sm:` fallback；8-metric
  行在小屏手机上缩成 1 列；把移动端"按钮占满宽"的规则限定到
  `.toolbar-shell` 内部，避免表格/卡片里的内联按钮被撑满；toast
  的 `min-width` 上限限制到视窗宽度；suffix-dialog 模态框加上
  外边距，手机上不再紧贴屏幕边缘。

#### 修复

- **UsageModule 查询按钮卡在 "查询中"** — admin `UsageModule` 和
  用户 `MyUsageModule` 的 summary 与 rows 两个 effect 共用同一个
  `queryTokenRef`。`setActiveQuery` 同时触发两个 effect 时，rows
  effect 在 summary 请求 resolve 之前就把 counter 递增了，于是
  summary 的 `.finally()` 检查（`queryTokenRef.current === token`）
  永远不成立，`setLoadingMeta(false)` 永远不会被调用，按钮就永远
  卡在"查询中"。拆成 `summaryTokenRef` + `rowsTokenRef`，让两个
  取消 token 各自独立，与 `useRequestsModuleState` 的做法对齐。
- **`x-title` 和 `http-referer` 透传到上游** — 在
  `gproxy-server/src/middleware/sanitize.rs` 和
  `sdk/gproxy-routing/src/sanitize.rs` 两处 header 黑名单里都加上
  这两项，让 OpenRouter 风格的客户端元数据不再到达可能会拒绝或
  记录它们的上游渠道。

#### 移除

- **`models` 表上的遗留 `price_each_call` + `price_tiers_json` 列**
  — 从 SeaORM 实体、`ModelQueryRow`、`ModelWrite`、
  `store_query/admin`、`write_sink`、`write/event` 中全部删除。
  定价只存在于 `pricing_json` 列里。Phase 2.3 → 2.4 过渡期间
  刻意把旧列留在磁盘上作为 backfill 的落脚点，本版本正式退役。
- **`update_source` 更新源配置** — TOML 字段、相关 i18n 文案、
  admin 类型，以及 `.github/workflows/release-binary.yml` 里
  内部更新服务器的流程全部删除。独立的 `DownloadsPage.astro`
  也一并删除，docs 里的下载链接改为指向 GitHub Releases。
- **孤儿前端 `ModelsModule`** — 该模块被
  `app/modules.tsx` 的 `activeModule` switch 通过
  `case "models"` 引入，但 `buildAdminNavItems` 从来没有为
  `"models"` 生成导航项，入口实际不可达。Admin 的模型管理已
  全部收敛到 provider 工作区的 Models Tab 里。
- **`gproxy-core` 中的 `PriceTier`** — 下游消费者改用
  `gproxy_sdk::provider::billing::ModelPriceTier`。

#### 兼容性

- **DB schema**：`models.pricing_json` 是一次纯加列变更，
  SeaORM 启动时的 schema-sync 会自动完成。已有行的值为 `NULL`，
  命中后会 fallback 到 provider 编译期内置的 `ModelPrice`。但
  **旧的 `price_each_call` 和 `price_tiers_json` 两列已从实体中
  移除** —— 如果你升级的是一个在这两列里仍有数据的 DB，请在切到
  v1.0.6 之前把这些数据迁移进 `pricing_json`。通过 TOML seed
  做干净安装的情况不受影响。
- **Admin 客户端**：upsert 请求体现在携带
  `pricing_json: string | null`。老字段 `price_each_call` /
  `price_tiers_json` 仍作为 nullable 保留在 admin API schema 上，
  但后端不再读取 —— 客户端请停止发送它们，改为发送
  `pricing_json`。
- **TOML 导出**：定价块里现在会带上 `flex` / `scale` / `priority`
  相关的新字段（如果填了的话）。不含这些字段的旧 TOML
  文件仍然可以干净地导入。
- **自更新源硬编码为 GitHub Releases**：`update_source` 配置项
  整块删除，部署方不能再把进程内自更新指向私有镜像或反向代理。
  就地升级本身仍然可用，硬编码从 `LeenHawk/gproxy` 的 GitHub
  Releases 拉取；之前依赖自定义镜像的部署需要改为带外更新
  二进制后重启进程，或者基于补丁后的下载基址自行重新编译
  gproxy。

## v1.0.5

> **Major refactor.** Two sibling releases worth of architectural cleanup
> condensed into one tag: the suffix system is deleted, the `models` and
> `model_aliases` DB tables are merged, rewrite-rule/billing ownership
> moves from the engine into the handler, and request-time model
> resolution finally makes `permission → rewrite → alias → execute`
> the single canonical order. No automated migration is shipped — old
> `model_aliases` rows are re-imported into the unified `models` table on
> startup when a TOML seed is present, otherwise re-enter them from the
> console once v1.0.5 is running.

### English

#### Added

- **Model aliases injected into `model_list` / `model_get` responses** — aliases
  are now first-class entries: they appear in the OpenAI / Claude / Gemini
  model-list responses (both scoped and unscoped) alongside real models,
  `GET /v1/models/{alias}` resolves to the alias, and non-stream responses
  have their `"model"` field rewritten to the alias name the client sent
  (streaming chunks are rewritten per chunk in the engine).
- **Suffix-aware alias resolution** — an alias like `gpt4-fast` is resolved
  by trying an exact match first, then stripping any known suffix from the
  tail, looking up the base alias, and re-appending the suffix before
  forwarding to the upstream model. *(Subsequently removed along with the
  whole suffix system, but the alias+suffix combo kept working via
  channel-level rewrite rules until then.)*
- **Unified model table** — `model_aliases` is merged into `models` with a
  new `alias_of: Option<i64>` column. A row with `alias_of = NULL` is a
  real model; a row with `alias_of = Some(id)` is an alias pointing at
  another row's id in the same table. The alias lookup structure
  (`HashMap<String, ModelAliasTarget>`) is unchanged — it is simply
  rebuilt from the unified `models` snapshot at startup / reload.
- **`POST /admin/models/pull`** — admin endpoint that fetches a provider's
  live model list from upstream and returns the model ids. The console
  uses this to populate the local `models` table via a new "Pull Models"
  button in the provider workspace's Models tab. Pulled models are
  imported as real entries (`alias_of = NULL`) with no pricing, which the
  admin can then edit.
- **Model List / Local dispatch for `model_list` / `model_get`** — the
  `*-only` dispatch template presets (chat-completions-only, response-only,
  claude-only, gemini-only) default model_list and model_get to the
  `Local` dispatch implementation. Requests served locally never hit
  upstream; the handler builds the protocol-specific response body
  directly from the `models` table. `GproxyEngine::is_local_dispatch(...)`
  lets handlers decide before calling `engine.execute`.
- **Local merge for non-Local dispatch** — for `*-like` / pass-through
  dispatch, the proxy still calls upstream for `model_list`, but the
  response is merged with the local `models` table before being returned:
  local real models that aren't in the upstream response get appended,
  then aliases mirror their target entry. `model_get` checks the local
  table first and returns the local entry if present, otherwise falls
  through to upstream. This works across OpenAI / Claude / Gemini
  protocols, scoped and unscoped.
- **Alias-level pricing fallback** — billing now tries to price a request
  against the alias name first and falls back to the resolved real model
  name if no alias-level pricing exists. Admins can set a custom
  `price_each_call` / `price_tiers_json` on an alias row to override the
  real model's pricing for that alias only.
- **Provider workspace: dedicated Rewrite Rules tab** — rewrite rules
  moved out of the Config tab's settings JSON editor into their own
  provider-workspace tab (`/providers/:name` → "Rewrite Rules"). The
  editor is a two-column list + detail layout: the left column shows all
  rules with a scrollbar (max ~10 visible), the right column shows path /
  action / JSON value / filter (model glob + operation / protocol chips)
  for the selected rule. Data still lives in `provider.settings_json`.
- **Provider workspace: unified "Models" tab** — the separate "Models"
  (pricing) and "Model Aliases" tabs are merged into a single "Models"
  tab that lists both real models and aliases in the same scrollable
  list. Alias rows are shown with an "alias" badge and a `→ target`
  indicator, and three filter buttons (All / Real only / Aliases only)
  control what is visible. The edit form has an `alias_of` dropdown for
  picking an alias target, and the pull-models flow is embedded in the
  same tab.
- **"+ Add Suffix Variant" dialog** in the Models tab — when a real
  model is selected, a new button opens a dialog that mirrors the old
  composable suffix system: the user picks one entry per group
  (thinking / reasoning / service tier / effort / verbosity / ...), the
  dialog computes a combined suffix string and a list of rewrite-rule
  actions, and on confirm it atomically creates an alias row
  (`alias_of = base.id`, `model_id = base + suffix`) and appends the
  rewrite rules to the provider's `settings_json` with
  `filter.model_pattern` scoped to the new alias name. Presets cover
  everything the deleted Rust suffix module supported except the Claude
  header-modifying suffixes (`-fast`, `-non-fast`, `-1m`, `-200k`),
  which rewrite rules can't express.
- **Rewrite rules editor: typed value input** — the "Set" action no
  longer forces admins to hand-write JSON. A Type dropdown
  (string / number / boolean / null / array / object) switches the
  value editor between a plain text input, numeric input, boolean
  dropdown, null placeholder, or JSON textarea (for arrays/objects).
  Switching type resets the value to a sensible default for the new
  type.
- **Rewrite rules editor: model-pattern autocomplete** — focusing the
  `model_pattern` input shows a scrollable dropdown of matching model
  names (real + aliases) for the current provider. Typing filters the
  list by substring without auto-completing the input; clicking an
  entry fills in the pattern exactly.
- **Pricing-by-alias in the billing pipeline** — the engine now exposes
  `build_billing_context` / `estimate_billing` as public methods, and the
  handler builds the billing context in the handler layer with the
  alias name visible so per-alias pricing takes effect.

#### Changed

- **Request pipeline ordering**: `permission check (original model name)
  → rewrite_rules (original model name) → alias resolve → engine.execute
  → billing`. Permission is checked against the name the client sent
  (before any alias rewrite), so admins must explicitly whitelist each
  alias — aliases do not silently inherit their target's permissions.
- **Rewrite rules moved out of the engine** into the handler layer. The
  engine no longer applies `rewrite_rules`; instead the handler calls
  `state.engine().rewrite_rules(provider)` and applies them to the
  request body itself, using the **original** model name for
  `model_pattern` matching so patterns like `gpt4-fast` can match before
  the name is rewritten by alias resolution.
- **Billing moved out of the engine** into the handler layer. The engine
  no longer computes cost / `billing_context` / `billing` on its
  `ExecuteResult`; those fields are gone. Handlers now call
  `engine.build_billing_context(...)` and `engine.estimate_billing(...)`
  directly after the upstream call returns, which is also what makes
  pricing-by-alias possible.
- **Provider proxy responses** rewrite the `"model"` field to the alias
  name the client sent, using the engine's new `response_model_override`
  field on `ExecuteRequest`. The suffix rewrite (when still present) was
  skipped when the alias override was about to overwrite the same field,
  avoiding a redundant JSON parse / serialize per request.
- **`model_alias_middleware` simplified** — the middleware now does a
  single exact alias lookup and drops the `ResolvedAlias.suffix` field;
  all suffix+alias combo handling has been removed along with the suffix
  system.

#### Fixed

- **`/admin/models/pull` returning HTTP 500** — the endpoint was cloning
  the admin request's headers (including `Authorization: Bearer
  <admin-token>`, `Content-Length`, `Host`) and forwarding them to the
  upstream, which either corrupted the body length or overrode the
  channel-supplied credentials. Pull now passes an empty `HeaderMap` so
  the channel's `finalize_request` is the only source of upstream
  headers. Error messages include the first 500 characters of the
  upstream response body so failures are debuggable.
- **Pull-models button was unreachable** — the button lived in the
  standalone `ModelAliasesModule` route, but the sidebar never linked to
  that route. Moved it into the provider-workspace Aliases tab (and
  eventually into the unified Models tab), where it actually renders.
- **Models tab scrolling** — the provider-workspace Models tab now has a
  `max-h-128` scrollable list so long model lists stay usable.
- **`custom` channel: `mask_table`** — the `mask_table` field was
  removed from the backend long ago, but the frontend custom-channel
  form still rendered a dead JSON editor. Removed from
  `channel-forms.ts`.

#### Removed

- **Suffix system** — the entire `sdk/gproxy-provider/src/suffix.rs`
  module (801 lines) is deleted, along with the `enable_suffix` field
  and `ChannelSettings::enable_suffix` / `ProviderRuntime::enable_suffix`
  methods on all 14 channels. Response / streaming suffix rewriting,
  suffix-based model-list expansion, the suffix groups, and all
  `match_suffix_groups` / `strip_model_suffix_in_body` /
  `rewrite_model_suffix_in_body` / `expand_model_list_with_suffixes` /
  `rewrite_model_get_suffix_in_body` helpers — gone. The same feature
  (`gpt4` vs `gpt4-fast` etc.) is now expressed as separate alias rows
  with channel-level rewrite rules.
- **`/admin/model-aliases/*` endpoints and `model_aliases` DB table** —
  deleted. All model and alias CRUD runs through `/admin/models/*`.
  `ModelQueryParams` gains an `alias_of_filter: "only_aliases" |
  "only_real" | null` to let the console restrict a query to one kind.
- **Standalone `ModelAliasesModule` route** — the orphaned
  `model-aliases` route and module are gone. The Models tab inside the
  provider workspace is the only place model rows are managed.

#### Compatibility

- **DB schema**: adding `alias_of` to `models` is a pure column add and
  is picked up automatically by the SeaORM schema-sync step on startup.
  The old `model_aliases` table is **not** dropped — if you upgrade
  against an existing v1.0.4 database the old rows stay in place but
  become dead weight; re-enter any aliases you want to keep via the
  console's Models tab after upgrading. A clean install via TOML seed
  seeds the new unified table directly.
- **Admin HTTP clients**: any client that was calling
  `/admin/model-aliases/*` must be updated to use `/admin/models/*`.
  Upsert payloads now carry an `alias_of: i64 | null` field; omit it
  (or pass `null`) for a real model.
- **Dispatch templates**: the `*-only` presets default to `Local`
  dispatch for `model_list` / `model_get`. Existing providers that were
  created before v1.0.5 keep whatever dispatch they had persisted in
  `provider.dispatch_json`; only newly created providers (or providers
  that explicitly re-apply a preset) get the new Local defaults. Pull
  models via the new button so the local `models` table has something
  to serve before clients hit those routes, or the response will be an
  empty list.
- **Suffix model names** (e.g. `gpt-4o-fast`, `claude-3-opus-thinking-high`)
  no longer work out of the box. Re-express them as explicit alias rows
  with per-channel rewrite rules that inject the relevant parameters
  into the request body.

### 中文

#### 新增

- **model_list / model_get 响应中注入模型别名** —
  别名现在是一等条目：它们会出现在 OpenAI / Claude / Gemini
  模型列表响应中（scoped 与 unscoped 同时适用），
  `GET /v1/models/{alias}` 会解析到该别名，非流式响应的
  `"model"` 字段会被改写为客户端发送的别名（流式响应由 engine
  在每个 chunk 中改写）。
- **Suffix-aware 的别名解析** — 形如 `gpt4-fast` 的别名会先尝试
  精确匹配，若未命中则从尾部剥离已知后缀、查找基础别名，再把后缀
  追加回解析后的真实模型名。*(该机制后来随整个 suffix 系统一并
  移除，改由渠道级 rewrite_rules 表达相同效果。)*
- **统一的 `models` 表** — 原先的 `model_aliases` 表合并进
  `models`，新增 `alias_of: Option<i64>` 列。`alias_of = NULL`
  代表真实模型，`alias_of = Some(id)` 代表别名、指向同表中另一行的
  id。内存中的别名查找结构（`HashMap<String, ModelAliasTarget>`）
  保持不变，只是数据来源改为在启动 / reload 时由统一的 `models`
  快照重新构建。
- **`POST /admin/models/pull`** — 新的 admin 接口，从上游拉取
  某个 provider 的实时模型列表并返回 model id。控制台用它在
  provider 工作区的 Models tab 里通过"拉取模型"按钮把结果导入
  本地 `models` 表。导入的模型默认是真实条目（`alias_of = NULL`）、
  不带价格，管理员可以再编辑补全。
- **`*-only` 调度模板下的 Local model_list / model_get** —
  `*-only` 预设模板（chat-completions-only、response-only、
  claude-only、gemini-only）把 `model_list` 与 `model_get` 默认
  调度改为 `Local`。被 Local 命中的请求完全不打上游，handler
  直接从 `models` 表按协议格式拼响应。新增
  `GproxyEngine::is_local_dispatch(...)` 让 handler 在调用
  `engine.execute` 之前就能判断。
- **非 Local 调度下的本地合并** — `*-like` / 直通调度下的
  `model_list` 仍然会打上游，但响应会与本地 `models` 表合并后再返
  回：上游响应中不存在的本地真实模型会被追加进来，随后别名条目
  再镜像它们的目标。`model_get` 则先查本地表，命中就直接返回本
  地条目，未命中再透传上游。OpenAI / Claude / Gemini 三种协议、
  scoped / unscoped 两种路径都生效。
- **按别名定价回退** — 计费先尝试用别名名查价格，若没有别名级
  定价再回退到真实模型名。管理员可以在别名行上单独设置
  `price_each_call` / `price_tiers_json` 来覆写真实模型的价格。
- **Provider 工作区：独立的"参数改写规则"Tab** — rewrite_rules
  从 Config Tab 的 settings JSON 编辑器里搬出来，独立成一个
  provider 工作区 Tab（`/providers/:name` → "参数改写规则"）。
  采用列表 + 详情的两栏布局：左列是所有规则（带滚动条，默认最多
  显示约 10 条），右列是选中规则的 path / action / JSON 值 /
  过滤条件（模型 glob + operation / protocol chip）。数据仍然
  落在 `provider.settings_json` 里。
- **Provider 工作区：统一的 Models Tab** — 原先的 "Models"
  （价格）和 "Model Aliases" 两个 Tab 合并为单一的 "Models" Tab，
  在同一个可滚动列表里同时展示真实模型和别名。别名条目显示
  "alias" 标签和 `→ 真实模型` 指示器，顶部有三个过滤按钮（全部
  / 仅真实模型 / 仅别名）。编辑表单新增 `alias_of` 下拉框用于
  选择别名指向的目标，拉取模型的流程也嵌入到同一个 Tab。
- **"+ 添加后缀变体"对话框** — Models Tab 里选中真实模型后，
  新按钮会打开一个对话框，对应旧的可组合 suffix 系统：用户在每个
  组（thinking / reasoning / service tier / effort / verbosity
  等）里挑一项，对话框合成出完整的后缀字符串和一组 rewrite_rules
  动作。确认后自动完成：创建别名行（`alias_of = 基础模型 id`，
  `model_id = 基础名 + 后缀`），并往该 provider 的 `settings_json`
  里追加带 `filter.model_pattern = 新别名` 的 rewrite_rules。预设
  覆盖了旧 Rust suffix 模块支持的所有配置，**但不包括** Claude 那
  几个改 header 的后缀（`-fast` / `-non-fast` / `-1m` / `-200k`），
  因为 rewrite_rules 只能改 body、不能改 header。
- **覆写规则编辑器：类型化值输入** — "Set/覆写" 动作不再强制手写
  JSON。新增"类型"下拉（string / number / boolean / null /
  array / object），按类型切换输入控件：字符串用普通文本输入框、
  数字用数字输入、布尔用 true/false 下拉、null 只显示占位提示，
  array / object 仍然用 JSON 编辑框。切换类型时会把值重置为该
  类型的默认值。
- **覆写规则编辑器：模型名自动补全** — `model_pattern` 输入框
  聚焦后弹出可滚动下拉列表，显示当前 provider 下所有模型名
  （真实 + 别名）。输入字符会按子串过滤列表，不会自动补全输入
  内容；点击下拉项会把完整模型名填进输入框。
- **计费管线支持按别名计价** — engine 现在把
  `build_billing_context` / `estimate_billing` 暴露为公开方法，
  handler 在 handler 层构造带有别名名的 billing context，让
  按别名定价真正生效。

#### 变更

- **请求管线顺序**：`权限检查（原始 model 名）→
  rewrite_rules（原始 model 名）→ 别名解析 → engine.execute →
  计费`。权限按客户端发送的名字检查（在任何别名改写之前），所以
  管理员必须为每个别名单独授权——别名不会默默继承其指向模型的
  权限。
- **Rewrite rules 移出 engine**，改由 handler 执行。engine 不再
  应用 `rewrite_rules`；handler 调用
  `state.engine().rewrite_rules(provider)` 然后自己把规则作用到
  请求体上，`model_pattern` 用**原始** model 名匹配，这样
  `gpt4-fast` 这样的 pattern 可以在名字被别名解析改写之前就命中。
- **计费移出 engine**，改由 handler 执行。engine 不再在
  `ExecuteResult` 上计算 `cost` / `billing_context` / `billing`，
  这些字段被移除。handler 现在在上游返回后直接调用
  `engine.build_billing_context(...)` 和
  `engine.estimate_billing(...)`，这也是实现按别名计价的前提。
- **代理响应** 会用 `ExecuteRequest.response_model_override` 把
  `"model"` 字段改写成客户端发送的别名。suffix 改写（在尚未移除
  之前）在别名即将覆盖同一字段时会被跳过，避免每个请求多一次
  无谓的 JSON 解析/序列化。
- **`model_alias_middleware` 简化** — 中间件现在只做一次精确
  别名查找，并且 `ResolvedAlias.suffix` 字段被移除；所有
  suffix+alias 组合处理逻辑随着 suffix 系统一起被删掉。

#### 修复

- **`/admin/models/pull` 返回 500** — 接口把 admin 请求的
  headers（包括 `Authorization: Bearer <admin-token>`、
  `Content-Length`、`Host`）原样 clone 后转发给上游，结果要么
  破坏 body 长度、要么覆盖掉 channel 本应注入的凭证。现在 pull
  只传一个空的 `HeaderMap`，让 channel 的 `finalize_request` 作
  为上游 headers 的唯一来源。错误消息也会带上游响应体的前 500
  个字符，方便排查。
- **拉取模型按钮不可达** — 按钮原先挂在独立的
  `ModelAliasesModule` 路由下，但侧边栏从未链接过该路由。现在
  按钮被挪到 provider 工作区的 Aliases Tab（最终合并到统一
  Models Tab）里，真正可见。
- **Models Tab 滚动** — provider 工作区的 Models Tab 列表现在
  带 `max-h-128` 的滚动容器，长模型列表也能正常使用。
- **`custom` 渠道：`mask_table`** — `mask_table` 字段早就从后端
  移除了，但前端 custom 渠道表单里仍然渲染了一个死掉的 JSON 编
  辑器。已从 `channel-forms.ts` 删除。

#### 移除

- **Suffix 系统** — 整个 `sdk/gproxy-provider/src/suffix.rs`
  模块（801 行）被删除，14 个 channel 上的 `enable_suffix` 字段
  和 `ChannelSettings::enable_suffix` /
  `ProviderRuntime::enable_suffix` 方法一并移除。响应/流式的
  suffix 改写、model_list 的 suffix 展开、suffix group 定义，
  以及 `match_suffix_groups` / `strip_model_suffix_in_body` /
  `rewrite_model_suffix_in_body` / `expand_model_list_with_suffixes`
  / `rewrite_model_get_suffix_in_body` 等辅助函数全部删除。
  同样的效果（`gpt4` 与 `gpt4-fast` 等）现在通过独立的别名行
  配合渠道级 rewrite_rules 表达。
- **`/admin/model-aliases/*` 端点和 `model_aliases` 表** — 删除。
  所有模型和别名的增删改查都走 `/admin/models/*`。`ModelQueryParams`
  新增 `alias_of_filter: "only_aliases" | "only_real" | null`
  供控制台按类型过滤。
- **独立的 `ModelAliasesModule` 路由** — 孤儿
  `model-aliases` 路由和模块已删除。provider 工作区里的 Models
  Tab 是管理模型行的唯一入口。

#### 兼容性

- **DB schema**：往 `models` 表加 `alias_of` 列是一次纯加列变更，
  启动时的 SeaORM schema-sync 会自动完成。旧的 `model_aliases`
  表**不会**被自动删除 —— 如果你是在已有 v1.0.4 数据库上升级，
  旧行会留在原位但变成死数据，想保留的别名请在升级后从控制台的
  Models Tab 重新录入。通过 TOML seed 做干净安装时，新的统一表
  会被直接 seed。
- **Admin HTTP 客户端**：调用 `/admin/model-aliases/*` 的客户端
  必须迁移到 `/admin/models/*`。Upsert 请求体现在携带
  `alias_of: i64 | null` 字段；真实模型填 `null` 或省略即可。
- **调度模板**：`*-only` 预设把 `model_list` / `model_get` 默认
  改为 `Local` 调度。升级前已经存在的 provider 仍然保留它们
  `provider.dispatch_json` 里持久化的调度规则；只有新建 provider
  （或显式重新应用预设的 provider）才会命中 Local 默认值。命中
  Local 之前请先用新按钮拉取模型，否则本地 `models` 表没数据，
  客户端拿到的会是空列表。
- **Suffix 风格的模型名**（例如 `gpt-4o-fast`、
  `claude-3-opus-thinking-high`）开箱即用的支持没了。请把它们改
  写成显式的别名行 + 渠道级 rewrite_rules，由规则把相应参数注入
  请求体。

## v1.0.4

### English

#### Added

- **Channel-level rewrite rules** — new `rewrite_rules` field on all 14
  channel Settings structs allows per-channel request body rewriting before
  the request is finalized. Rules support JSON path targeting with glob
  matching. A dedicated `RewriteRulesEditor` component with full i18n is
  available in the console.
- **Dispatch template presets for custom channel** — the console now offers
  built-in dispatch template presets when configuring custom channels,
  and dispatch templates are shown for all channel types (not just custom).

#### Fixed

- **Request log query button stuck on loading** — the query button no longer
  gets permanently stuck in loading state.
- **HTTP client protocol negotiation** — removed `http1_only` restriction and
  enabled proper HTTP/1.1 support for client builders, improving compatibility
  with upstream providers behind HTTP/1.1-only proxies.
- **Sampling parameter stripping** — model-aware stripping for
  anthropic/claudecode channels ensures unsupported sampling parameters are
  correctly removed based on the target model.
- **Dispatch template passthrough** — `*-only` dispatch templates now correctly
  use passthrough+transform for `model_list` / `model_get` operations.
- **Session-expired toast suppressed** — the error toast for expired sessions
  is now suppressed before the page reload, preventing a flash of error UI.
- **Update-available toast color** — changed from error-red to green success
  style.
- **Noisy ORM logging** — `sqlx` and `sea_orm` log levels now default to
  `warn`, reducing log noise at startup and during normal operation.
- **Dispatch / sanitize rules overflow** — both panels now scroll when content
  exceeds the viewport instead of overflowing the layout.
- **Upstream proxy placeholder** — the upstream proxy input field now shows a
  placeholder hint.
- **Frontend i18n** — `alias`, `enable_suffix`, `enable_magic_cache` labels
  are now properly translated; "模型" renamed to "模型价格表" / "Model Pricing";
  `sanitize_rules` renamed to "消息重写规则" / "Message Rewrite Rules".

---

### 中文

#### 新增

- **渠道级重写规则** — 全部 14 个渠道 Settings 结构新增 `rewrite_rules`
  字段，支持在请求最终发送前对请求体进行按路径重写，规则支持 JSON path
  定位与 glob 匹配。控制台提供专用的 `RewriteRulesEditor` 结构化编辑组件，
  完整支持中英文。
- **Custom 渠道调度模板预设** — 控制台在配置 custom 渠道时提供内置调度模板
  预设，且调度模板现在对所有渠道类型可见（不再限于 custom）。

#### 修复

- **请求日志查询按钮卡死** — 查询按钮不再永久停留在 loading 状态。
- **HTTP 客户端协议协商** — 移除 `http1_only` 限制并启用 HTTP/1.1 支持，
  改善通过仅支持 HTTP/1.1 的代理访问上游 provider 的兼容性。
- **采样参数裁剪** — anthropic/claudecode 渠道现在根据目标模型感知地裁剪
  不支持的采样参数。
- **调度模板透传** — `*-only` 调度模板现在正确使用 passthrough+transform
  处理 `model_list` / `model_get` 操作。
- **会话过期 toast 抑制** — 页面刷新前不再闪现会话过期的错误提示。
- **更新可用 toast 颜色** — 从红色错误样式改为绿色成功样式。
- **ORM 日志降噪** — `sqlx` 和 `sea_orm` 日志级别默认设为 `warn`，减少
  启动和运行期间的日志噪音。
- **调度规则 / 重写规则溢出** — 两个面板内容超出视口时改为滚动，不再
  撑破布局。
- **上游代理占位提示** — 上游代理输入框现在显示占位符提示。
- **前端国际化** — `alias`、`enable_suffix`、`enable_magic_cache` 标签
  已正确翻译；"模型"改名为"模型价格表" / "Model Pricing"；`sanitize_rules`
  改名为"消息重写规则" / "Message Rewrite Rules"。

## v1.0.3

### English

#### Added

- **Suffix system for model-list / model-get** — suffix modifiers (e.g. `-thinking-high`, `-fast`) are now expanded in model list responses and rewritten in model get responses, so clients can discover available suffix variants.
- **Suffix per-channel toggle** — new `enable_suffix` setting lets operators enable/disable suffix processing per channel.
- **VertexExpress local model catalogue** — model list/get requests are served from a static model catalogue embedded at compile time, since Vertex AI Express does not expose a standard model-listing endpoint.
- **Vertex SA token bootstrap on credential upsert** — when a Vertex credential with `client_email` and `private_key` is added via the admin API, the access token is automatically obtained so the first request has valid auth.

#### Fixed

- **GeminiCLI / Antigravity model list** — both channels now correctly route model list/get through their respective quota/model endpoints (`retrieveUserQuota` for GeminiCLI, `fetchAvailableModels` for Antigravity) and normalize responses to standard Gemini format.
- **Vertex model list normalization** — Vertex AI returns `publisherModels` with full resource paths; responses are now converted to standard Gemini `models` format.
- **Vertex / VertexExpress header filtering** — `anthropic-version` and `anthropic-beta` headers are dropped before forwarding to Google endpoints.
- **Vertex GeminiCLI-style User-Agent** — Vertex requests now send proper `User-Agent` and `x-goog-api-client` headers matching Gemini CLI traffic.
- **Engine HTTP client proxy** — database proxy settings now take effect after bootstrap; previously the engine client was built before DB config was loaded.
- **Engine HTTP/1.1 for standard client** — the non-spoof wreq client uses `http1_only()` for reliable proxy traversal.
- **HTTP client request dispatch** — switched from `wreq::Request::from() + execute()` to `client.request().send()` to ensure proxy/TLS settings propagate correctly.
- **Frontend: VertexExpress credential** — field changed from `access_token` to `api_key`.
- **Frontend: Vertex credential** — added missing optional fields (`private_key_id`, `client_id`, `token_uri`).

---

### 中文

#### 新增

- **Suffix 系统支持 model-list / model-get** — suffix 修饰符（如 `-thinking-high`、`-fast`）现在会在模型列表响应中展开、在模型详情响应中回写，客户端可以发现可用的 suffix 变体。
- **Suffix 按渠道开关** — 新增 `enable_suffix` 配置项，可按渠道启用/禁用 suffix 处理。
- **VertexExpress 本地模型目录** — model list/get 请求从编译时嵌入的静态模型目录返回，因为 Vertex AI Express 没有标准的模型列表端点。
- **Vertex SA 凭证 upsert 自动换 token** — 通过 admin API 添加包含 `client_email` 和 `private_key` 的 Vertex 凭证时，自动获取 access token，首次请求不会因空 token 失败。

#### 修复

- **GeminiCLI / Antigravity 模型列表** — 两个渠道现在正确通过各自的配额/模型端点（GeminiCLI 用 `retrieveUserQuota`，Antigravity 用 `fetchAvailableModels`）路由 model list/get 请求，并将响应整形为标准 Gemini 格式。
- **Vertex 模型列表整形** — Vertex AI 返回的 `publisherModels`（含完整资源路径）现在被转换为标准 Gemini `models` 格式。
- **Vertex / VertexExpress 头过滤** — 转发到 Google 端点前丢弃 `anthropic-version` 和 `anthropic-beta` 头。
- **Vertex GeminiCLI 风格 User-Agent** — Vertex 请求现在发送匹配 Gemini CLI 流量的 `User-Agent` 和 `x-goog-api-client` 头。
- **Engine HTTP 客户端代理** — 数据库代理设置现在在自举后生效；之前 engine 客户端在 DB 配置加载前就已构建。
- **Engine 标准客户端 HTTP/1.1** — 非伪装 wreq 客户端使用 `http1_only()` 确保代理穿透可靠。
- **HTTP 客户端请求调度** — 从 `wreq::Request::from() + execute()` 改为 `client.request().send()`，确保代理/TLS 设置正确传递。
- **前端：VertexExpress 凭证** — 字段从 `access_token` 改为 `api_key`。
- **前端：Vertex 凭证** — 添加缺失的可选字段（`private_key_id`、`client_id`、`token_uri`）。

## v1.0.2

### English

#### Added

- **WebSocket per-model usage tracking** — when the client switches models mid-session (e.g. via `response.create`), usage is segmented per model and recorded separately instead of attributing all tokens to the last model.
- **WebSocket upstream message logging** — WS session end now records an upstream request log containing all client→server and server→client messages as request/response body.

---

### 中文

#### 新增

- **WebSocket 按模型分段用量** — 客户端在 WS 会话中切换模型时，用量按模型分段记录，不再把所有 token 归到最后一个模型。
- **WebSocket 上游消息日志** — WS session 结束时记录上游请求日志，包含所有客户端→服务器和服务器→客户端消息。

## v1.0.1

### English

#### Added

- **Upstream request logging** — quota queries and cookie exchange HTTP steps
  are now recorded in the `upstream_requests` table, giving full visibility
  into every outbound call the proxy makes.
- **Streaming body capture** — both downstream and upstream logs now defer
  recording until the stream ends, so `response_body` is populated for
  streaming requests. Controlled by `enable_downstream_log_body` /
  `enable_upstream_log_body` config.
- **Auto-check for updates** — the console fires a background version check
  after admin login and shows a toast when a new release is available.
- **Wildcard model permission for admins** — creating or promoting a user to
  admin now automatically seeds a `*` model permission so the admin can call
  all providers immediately.
- **Credential import via raw JSON** — the console credential form now offers
  a single JSON textarea for direct paste import; plain cookie or API-key
  strings are auto-wrapped into the correct JSON shape.

#### Fixed

- **Credential token refresh persisted** — refreshed `access_token` values
  (from `refresh_token` on 401/403) are now written back to the database and
  updated in memory, so they survive restarts.
- **Cookie-only credentials** — credentials with only a `cookie` field (no
  `access_token`) can now be deserialized; bootstrap populates the token.
- **Claude Code org info backfill** — `billing_type`, `rate_limit_tier`,
  `account_uuid`, and `user_email` are now extracted from the bootstrap
  /organizations response when the token endpoint omits them.
- **Version check endpoint** — the updater now uses the GitHub Releases API
  instead of a nonexistent `latest.json` manifest URL.
- **Console session stability** — 401 responses from upstream provider routes
  no longer incorrectly clear the admin session; only `/admin/*` and `/login`
  401s trigger logout.
- **Request log loading loop** — removed `pageCursors` from the row-loading
  effect dependency array to break an infinite re-render cascade.
- **Cache breakpoint TTL aliases** — `"5m"` and `"1h"` are now accepted as
  serde aliases alongside `"ttl5m"` / `"ttl1h"`.
- **Credential quota reset time** — displayed in local timezone via
  `toLocaleString()` instead of raw ISO strings.
- **Credential card layout** — title, badge, and action buttons now wrap
  cleanly; expanded details flow below on their own line.
- **Android CI** — updated `setup-android` action to v4.

#### Changed

- **`subscription_type` removed** — `subscription_type` / `billing_type` /
  `organization_type` fields are dropped from credential, cookie exchange,
  OAuth profile, and frontend forms. Only `rate_limit_tier` is retained.
- **Cache breakpoint simplification** — `content_position` / `content_index`
  removed from breakpoint rules; breakpoints now always use flat block
  positioning across all messages.
- **i18n** — shortened Chinese cache breakpoint position labels
  (正数 / 倒数).

### 中文

#### 新增

- **上游请求日志** — 配额查询和 cookie 交换的每一步 HTTP 请求现在都会记录到
  `upstream_requests` 表，完整追踪代理发出的所有出站调用。
- **流式响应 body 采集** — 下游和上游日志均推迟到流结束后再写入，流式请求的
  `response_body` 不再为空。由 `enable_downstream_log_body` /
  `enable_upstream_log_body` 配置控制。
- **自动检查更新** — 管理员登录后控制台会在后台检查新版本，有新版时弹出提示。
- **管理员自动授权通配符模型权限** — 新建或提升为 admin 的用户会自动获得 `*`
  模型权限，无需手动配置即可调用所有 provider。
- **凭证 JSON 粘贴导入** — 控制台凭证表单新增单个 JSON 文本框，支持直接粘贴
  完整 JSON；也可粘贴纯 cookie 或 API key 字符串，自动包装为正确格式。

#### 修复

- **凭证 token 刷新落库** — 通过 refresh_token 刷新的 access_token 现在会
  同时更新内存和写入数据库，重启后不丢失。
- **纯 cookie 凭证** — 仅含 `cookie` 字段（无 `access_token`）的凭证现在可以
  正常反序列化，bootstrap 流程会自动补全 token。
- **Claude Code 组织信息回填** — 当 token 端点未返回组织信息时，
  `billing_type`、`rate_limit_tier`、`account_uuid`、`user_email` 会从
  bootstrap /organizations 响应中提取并回填。
- **版本检查端点** — 更新检查改用 GitHub Releases API，不再请求不存在的
  `latest.json`。
- **控制台会话稳定性** — 上游 provider 路由返回的 401 不再误触发管理员登出，
  仅 `/admin/*` 和 `/login` 路径的 401 才清除会话。
- **请求日志加载死循环** — 从行加载 effect 的依赖数组中移除 `pageCursors`，
  打破无限重渲染循环。
- **缓存断点 TTL 别名** — `"5m"` 和 `"1h"` 现在可以作为 serde 别名使用，
  与 `"ttl5m"` / `"ttl1h"` 等效。
- **凭证配额重置时间** — 使用 `toLocaleString()` 显示本地时区，不再显示原始
  ISO 字符串。
- **凭证卡片布局** — 标题、标记和操作按钮正确换行；展开详情独占整行显示。
- **Android CI** — `setup-android` action 升级到 v4。

#### 变更

- **移除 `subscription_type`** — 从凭证、cookie 交换、OAuth profile 和前端
  表单中删除 `subscription_type` / `billing_type` / `organization_type`
  字段，仅保留 `rate_limit_tier`。
- **缓存断点简化** — 移除 breakpoint 规则中的 `content_position` /
  `content_index`，断点统一使用跨所有消息的扁平 block 定位。
- **国际化** — 缩短中文缓存断点位置标签（正数 / 倒数）。

## v1.0.0

> **Breaking release.** gproxy v1.0.0 is a full ground-up rewrite of the v0.3.x
> line. Treat it as a brand-new project: the workspace layout, storage schema,
> HTTP API, admin surface, TOML config format, CLI flags, and provider settings
> have all changed and are **not** compatible with v0.3.42 or earlier. There is
> no in-place upgrade path — see the Compatibility section below.

### English

#### Added

- Brand-new workspace layout split into three layers:
  - `sdk/` — protocol & provider SDK: `gproxy-sdk`, `gproxy-protocol`,
    `gproxy-provider`, `gproxy-routing`. Handles protocol conversion, provider
    execution, credential health, routing and cache affinity.
  - `crates/` — app layer: `gproxy-core`, `gproxy-storage`, `gproxy-api`,
    `gproxy-server`. Owns HTTP routing, admin API, storage, and `AppState`.
  - `apps/` — binaries: `gproxy` (main server) and `gproxy-recorder`
    (standalone MITM recorder).
- New storage layer (`gproxy-storage`) built on SeaORM + SQLx with first-class
  support for **SQLite**, **PostgreSQL**, and **MySQL**. Schema is auto-synced
  on startup; DSN is passed via `GPROXY_DSN` or persisted in `global_settings`.
- New embedded browser console mounted at `/console` (built from
  `frontend/console`, shipped inside the binary via `rust-embed`). Browser
  login flow: `POST /login` → session API key → `Authorization: Bearer`.
- New admin HTTP API under `/admin/*` with query/upsert/delete/batch handlers
  for providers, credentials, credential statuses, models, model aliases,
  users, user keys, user permissions, file permissions, rate limits, quotas,
  global settings, request logs, usage logs, config export, and self-update.
- New user HTTP API under `/user/*` for self-service key management, quota
  lookup, and usage queries.
- New provider proxy surface with both **scoped** (`/{provider}/v1/...`) and
  **unscoped** (`/v1/...`) routes, covering Claude Messages, OpenAI Chat
  Completions, OpenAI Responses, Embeddings, Images, Models, Gemini v1beta,
  and provider file APIs (upload/list/get/delete/content).
- New WebSocket bridging: passthrough, OpenAI ↔ Gemini Live, and Gemini Live ↔
  OpenAI Responses, via `ws_bridge.rs`.
- Security hardening:
  - Password hashing uses **Argon2id** with explicit OWASP parameters
    (19 MiB memory, 2 iterations).
  - API keys are stored as **SHA-256 digests** with constant-time comparison
    to defeat timing attacks.
  - Optional **field-level encryption** for credentials / passwords / API keys
    via `DATABASE_SECRET_KEY` (XChaCha20Poly1305).
  - Admin API responses **mask credential secrets** so raw tokens never
    leak back out.
- Optional multi-instance backend via the `redis` Cargo feature. When the
  binary is built with `--features redis` and `GPROXY_REDIS_URL` is set,
  rate limiting, quota reservation, and cache affinity are dispatched to
  Redis (Lua-atomic INCR / reserve / settle) instead of the local process.
- New TOML seed-config format (`gproxy.example.toml` / `gproxy.example.full.toml`)
  driving first-time bootstrap via `[global]`, `[[providers]]`, `[[models]]`,
  `[[model_aliases]]`, `[[users]]`, `[[users.keys]]`, `[[permissions]]`,
  `[[file_permissions]]`, `[[rate_limits]]`, `[[quotas]]`.
- New standalone `gproxy-recorder` binary under `apps/gproxy-recorder` for
  capturing upstream LLM traffic independently of the main server.
- Graceful shutdown pipeline: Axum `with_graceful_shutdown`, a bounded worker
  set draining within 5s, `UsageSink` final flush, `HealthBroadcaster` debounce
  flush, and exit hooks for `QuotaReconciler` / `RateLimitGC`.

#### Changed

- Workspace version bumped from `0.3.42` to **`1.0.0`**.
- Workspace members rewritten from a flat `crates/` list (`gproxy-admin`,
  `gproxy-middleware`, `gproxy-protocol`, `gproxy-provider`, `gproxy-core`,
  `gproxy-storage`) to the new `sdk/ + crates/ + apps/` layout described above.
  The old `gproxy-admin` and `gproxy-middleware` crates no longer exist.
- All provider execution now goes through `gproxy-sdk`'s `GproxyEngine`.
  Provider registration, credential dispatch, protocol conversion, and cache
  affinity are owned by the SDK; the app layer only orchestrates HTTP,
  storage, and `AppState`.
- Admin mutations follow an explicit **DB-first** model: write storage →
  sync `AppState` → rebuild `GproxyEngine` atomically via `ArcSwap`. Hot
  reload is exposed through `POST /admin/reload`.
- Read paths are **memory-first**: auth, permission checks, rate limiting,
  quota checks and alias resolution all run out of `AppState` `ArcSwap` /
  `DashMap` structures. The database is no longer on the request hot path.
- Bootstrap precedence: existing DB → TOML seed (`GPROXY_CONFIG`) →
  built-in defaults. Seed is persisted before being loaded into memory so
  `reload_from_db()` fully reconstructs state after a crash.
- CLI / environment variables have been reworked around the new app. The
  full set lives in `apps/gproxy/src/main.rs` and includes `GPROXY_HOST`,
  `GPROXY_PORT`, `GPROXY_ADMIN_USER`, `GPROXY_ADMIN_PASSWORD`,
  `GPROXY_ADMIN_API_KEY`, `GPROXY_DSN`, `GPROXY_CONFIG`, `GPROXY_DATA_DIR`,
  `GPROXY_PROXY`, `GPROXY_SPOOF`, `DATABASE_SECRET_KEY`, and (with the
  `redis` feature) `GPROXY_REDIS_URL`.
- Credential health is now managed by the SDK at runtime and snapshotted
  into a dedicated `credential_statuses` table; bootstrap restores the
  snapshot back into SDK memory.
- Release validation script now runs workspace-wide `cargo fmt` +
  `cargo clippy --workspace --all-targets -- -D warnings
  -A clippy::too_many_arguments` before tagging.

#### Removed

- The entire v0.3.x admin UI, provider settings schema, and channel-specific
  toggles. Legacy fields like `claudecode_enable_billing_header`,
  `claudecode_flatten_system_text_before_cache_control`,
  `enable_claude_1m_sonnet` / `enable_claude_1m_opus`, `priority_tier`,
  `claudecode_extra_beta_headers`, etc. are **not** carried over. Any of
  these behaviors you need to retain must be re-expressed against the new
  v1 provider/credential schema.
- Legacy v0.3.x storage tables, write-event variants, and on-disk layout.
  There is no automated migration from v0.3.x SQLite / MySQL / Postgres
  databases to the v1 schema.
- Old `crates/gproxy-admin` and `crates/gproxy-middleware` crates. Their
  responsibilities are now split across `gproxy-api`, `gproxy-server`, and
  the `sdk/` crates.
- Legacy per-channel credential status semantics — the new `gproxy-sdk`
  classifies failures (transient vs dead vs cooldown) uniformly across
  providers, so previously tuned per-channel fallbacks (e.g. Claude 1M
  automatic retry, Codex `402 deactivated_workspace` handling) are replaced
  by the SDK's unified health model.
- Old `release.sh` assumptions about a flat `crates/` layout. See the new
  script at `release.sh`.

#### Compatibility

- **This is a hard break from v0.3.x.** No automated migration path is
  provided. Plan to stand up a fresh database, regenerate admin and user
  credentials, and re-enter providers / models / aliases / permissions /
  quotas against the new v1 schema.
- Old `gproxy.toml` files from v0.3.x will not load as-is. Rewrite them
  against `gproxy.example.toml` / `gproxy.example.full.toml` before
  starting v1.
- HTTP clients that previously called v0.3.x admin routes must be updated
  to the new `/admin/*` query/upsert/delete/batch surface; request and
  response payloads have changed.
- User-facing provider proxy routes (`/v1/...`, `/{provider}/v1/...`) are
  compatible at the protocol level with standard Claude / OpenAI / Gemini
  clients, but auth, model aliasing, and permission errors are returned
  through the v1 error shape — downstream integrations that parsed v0.3.x
  error payloads should be reviewed.
- Credential secrets, passwords, and API keys should be re-imported after
  `DATABASE_SECRET_KEY` has been decided. Switching `DATABASE_SECRET_KEY`
  later is not a supported in-place operation.
- Multi-instance deployments that relied on the v0.3.x in-process counters
  must now opt into the `redis` feature and point `GPROXY_REDIS_URL` at a
  shared Redis instance; otherwise rate limit and quota state remain
  per-process.
- `gproxy-recorder` is independent from the main binary. If you previously
  used an ad-hoc recording setup in v0.3.x, switch to the new recorder app.

### 中文

#### 新增

- 全新的三层 workspace 布局：
  - `sdk/` — 协议与 provider SDK：`gproxy-sdk`、`gproxy-protocol`、
    `gproxy-provider`、`gproxy-routing`。负责协议转换、provider 执行、凭证
    健康、路由与缓存亲和。
  - `crates/` — 应用层：`gproxy-core`、`gproxy-storage`、`gproxy-api`、
    `gproxy-server`。负责 HTTP 路由、admin API、存储与 `AppState`。
  - `apps/` — 可执行程序：`gproxy`（主服务）与 `gproxy-recorder`
    （独立的 MITM 录制工具）。
- 全新的存储层（`gproxy-storage`），基于 SeaORM + SQLx，原生支持
  **SQLite**、**PostgreSQL**、**MySQL**。启动时会自动同步 schema；DSN 可
  通过 `GPROXY_DSN` 传入，或从 `global_settings` 中恢复。
- 全新的嵌入式浏览器控制台，挂载在 `/console`（由 `frontend/console`
  构建，通过 `rust-embed` 打入二进制）。浏览器登录流程：`POST /login` →
  会话 API key → `Authorization: Bearer`。
- 全新的 admin API：`/admin/*` 下统一提供 providers、credentials、
  credential statuses、models、model aliases、users、user keys、用户权限、
  文件权限、限流、配额、全局设置、请求日志、用量日志、配置导出与自更新的
  query / upsert / delete / batch 接口。
- 全新的 user API：`/user/*`，供用户自助管理 API key、查询配额与用量。
- 全新的 provider 代理入口，同时提供 **scoped**（`/{provider}/v1/...`）
  与 **unscoped**（`/v1/...`）两种路径，覆盖 Claude Messages、OpenAI Chat
  Completions、OpenAI Responses、Embeddings、Images、Models、Gemini
  v1beta，以及 provider 文件 API（上传/列出/查询/删除/下载）。
- 全新的 WebSocket 桥接：同协议透传、OpenAI ↔ Gemini Live、Gemini Live ↔
  OpenAI Responses，统一由 `ws_bridge.rs` 实现。
- 安全加固：
  - 用户密码统一使用 **Argon2id**，并按 OWASP 建议固定参数
    （19 MiB 内存、2 次迭代）。
  - API key 以 **SHA-256 摘要** 方式存储，使用常量时间比对，防御计时
    攻击。
  - 可选的字段级加密，通过 `DATABASE_SECRET_KEY`
    （XChaCha20Poly1305）加密凭证 / 密码 / API key。
  - Admin API 返回时会 **脱敏凭证密钥**，上游 token 不会再原样回吐。
- 可选的多实例后端：`redis` Cargo feature。当启用 `--features redis`
  并设置 `GPROXY_REDIS_URL` 时，限流、配额预留和缓存亲和会通过 Redis
  Lua 原子脚本完成，不再依赖单进程本地状态。
- 全新的 TOML 种子配置（`gproxy.example.toml` / `gproxy.example.full.toml`），
  用于首次启动时初始化 DB：`[global]`、`[[providers]]`、`[[models]]`、
  `[[model_aliases]]`、`[[users]]`、`[[users.keys]]`、`[[permissions]]`、
  `[[file_permissions]]`、`[[rate_limits]]`、`[[quotas]]`。
- 独立的 `gproxy-recorder` 可执行程序（`apps/gproxy-recorder`），用于
  脱离主服务独立抓取上游 LLM 流量。
- 优雅关闭流水线：Axum `with_graceful_shutdown`、5 秒封顶的 worker
  收敛、`UsageSink` 终态刷写、`HealthBroadcaster` 去抖 flush，以及
  `QuotaReconciler` / `RateLimitGC` 的退出钩子。

#### 变更

- workspace 版本由 `0.3.42` 升级到 **`1.0.0`**。
- workspace 成员从旧的扁平 `crates/` 列表（`gproxy-admin`、
  `gproxy-middleware`、`gproxy-protocol`、`gproxy-provider`、`gproxy-core`、
  `gproxy-storage`）重构为上述 `sdk/ + crates/ + apps/` 三层布局。
  原有的 `gproxy-admin`、`gproxy-middleware` crate 已经不再存在。
- 所有 provider 执行现在都通过 `gproxy-sdk` 的 `GproxyEngine`。provider
  注册、凭证调度、协议转换与缓存亲和由 SDK 掌握；app 层只负责编排 HTTP、
  存储与 `AppState`。
- Admin mutation 遵循明确的 **DB-first** 模型：先写存储 → 同步 `AppState`
  → 通过 `ArcSwap` 原子替换 `GproxyEngine`。热重载通过
  `POST /admin/reload` 暴露。
- 读路径为 **Memory-first**：鉴权、权限、限流、配额检查、别名解析等全部
  走 `AppState` 的 `ArcSwap` / `DashMap`。数据库不再出现在请求热路径上。
- Bootstrap 优先级：已有 DB → TOML 种子（`GPROXY_CONFIG`）→ 内置默认。
  种子会先落 DB 再加载到内存，保证崩溃后 `reload_from_db()` 能够完整恢复。
- CLI / 环境变量围绕新应用重新梳理。完整列表见
  `apps/gproxy/src/main.rs`，包括 `GPROXY_HOST`、`GPROXY_PORT`、
  `GPROXY_ADMIN_USER`、`GPROXY_ADMIN_PASSWORD`、`GPROXY_ADMIN_API_KEY`、
  `GPROXY_DSN`、`GPROXY_CONFIG`、`GPROXY_DATA_DIR`、`GPROXY_PROXY`、
  `GPROXY_SPOOF`、`DATABASE_SECRET_KEY`，以及启用 `redis` feature 后的
  `GPROXY_REDIS_URL`。
- 凭证健康状态现在由 SDK 在运行时维护，并定期快照到 `credential_statuses`
  表；bootstrap 时再从快照恢复回 SDK 内存。
- 发版校验脚本现在会在打 tag 前运行 workspace 级 `cargo fmt` +
  `cargo clippy --workspace --all-targets -- -D warnings
  -A clippy::too_many_arguments`。

#### 移除

- 整套 v0.3.x 的后台 UI、provider 设置结构与渠道专用开关。旧字段如
  `claudecode_enable_billing_header`、
  `claudecode_flatten_system_text_before_cache_control`、
  `enable_claude_1m_sonnet` / `enable_claude_1m_opus`、`priority_tier`、
  `claudecode_extra_beta_headers` 等 **均未保留**。如果需要这些行为，请
  按 v1 新的 provider / credential schema 重新表达。
- v0.3.x 的存储表结构、write-event 变体以及落盘布局。不提供从 v0.3.x
  SQLite / MySQL / Postgres 到 v1 schema 的自动迁移。
- 旧的 `crates/gproxy-admin`、`crates/gproxy-middleware` crate。其职责
  已拆分到 `gproxy-api`、`gproxy-server` 及 `sdk/` 下。
- 老版本按渠道定制的凭证健康语义——新的 `gproxy-sdk` 会跨 provider 统一
  分类失败（瞬时 / dead / cooldown），因此之前针对特定渠道的降级逻辑
  （如 Claude 1M 自动重试、Codex `402 deactivated_workspace` 专用处理）
  均被统一的 SDK 健康模型取代。
- 原先假设扁平 `crates/` 布局的 `release.sh`。请使用仓库根目录下的新
  `release.sh`。

#### 兼容性

- **这是相对 v0.3.x 的硬断代。** 不提供任何自动迁移路径。请按全新项目
  对待：新建数据库，重新生成 admin / user 凭证，并在 v1 schema 下重新
  配置 providers / models / aliases / permissions / quotas。
- v0.3.x 时代的 `gproxy.toml` 无法直接加载。请参照
  `gproxy.example.toml` / `gproxy.example.full.toml` 重新编写后再启动 v1。
- 依赖 v0.3.x admin 路由的 HTTP 客户端必须全面迁移到新的 `/admin/*`
  query / upsert / delete / batch 接口；请求与响应 payload 均已变更。
- 面向用户的 provider 代理路由（`/v1/...`、`/{provider}/v1/...`）在协议
  层仍兼容标准 Claude / OpenAI / Gemini 客户端；但鉴权、模型别名、权限
  等错误会按 v1 的错误结构返回，下游如果此前解析过 v0.3.x 错误体，请
  重新核对。
- 凭证密钥、用户密码、API key 应在确定 `DATABASE_SECRET_KEY` 之后再
  重新导入。运行后再切换 `DATABASE_SECRET_KEY` 不是受支持的原地操作。
- 依赖 v0.3.x 进程内限流 / 配额计数的多实例部署，必须启用 `redis`
  feature 并把 `GPROXY_REDIS_URL` 指向共享 Redis；否则限流与配额仍然
  是进程级的。
- `gproxy-recorder` 独立于主服务。如果此前在 v0.3.x 使用了临时的录制
  方案，请迁移到新的 recorder 二进制。