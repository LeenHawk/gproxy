# Release Notes

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