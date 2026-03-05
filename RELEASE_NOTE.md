# Release Notes

## v0.3.22

### Added

- Added signed self-update verification for release assets:
  - updater now resolves `<asset>.sha256` and `<asset>.sha256.sig`
  - checksum signatures are verified with Ed25519 before binary install.
- Added web-hosted self-update source support:
  - updater can read channel manifests from `<downloads-base>/{releases|staging}/manifest.json`
  - manifest assets can provide `sha256`, `sha256_url`, `sha256_sig_url`, `key_id`.
- Added raw payload execution path for provider routes:
  - providers now support `*_payload_with_retry` flow for raw-lane passthrough requests without typed decode.
- Added explicit SeaORM create mutations:
  - `create_provider`, `create_credential`, `create_user`, `create_user_key`.
- Added CI workflow for repository quality checks:
  - Rust: `cargo fmt --check`, `cargo clippy`, `cargo test`
  - frontend: `pnpm typecheck`, `pnpm lint`, `pnpm test`.
- Added Starlight docs `ThemeSelect` override to stabilize auto/light/dark switching behavior.

### Changed

- Updated global update-source model and defaults:
  - canonical values are now `github` and `web-hosted`
  - default `update_source` changed to `github` in runtime defaults, storage defaults, examples, and admin UI.
- Updated Antigravity upstream shaping for project-scoped forward requests:
  - wrapped request payload now injects `request.sessionId`
  - upstream request headers now include `x-machine-session-id`
  - `sessionId` is deterministically derived from `credential_id + project_id` to keep stable cache behavior across retries and process restarts
  - non-project routes (such as model list/get, embedding, and usage reporting) remain unchanged.
- Updated admin create/upsert APIs to use storage-assigned IDs:
  - `/admin/providers/upsert`, `/admin/credentials/upsert`, `/admin/users/upsert` now accept optional `id` and return `{ ok, id }`
  - Admin UI now treats IDs as backend-assigned (ID inputs are read-only during create flows).
- Updated user key generation flows to avoid in-memory incremental IDs:
  - login auto-key generation, `/admin/user-keys/generate`, and `/user/keys/generate` now create rows directly in DB with unique-retry handling.
- Updated TOML import behavior for providers/credentials:
  - missing provider/credential rows are created directly in storage (instead of synthetic incremental IDs)
  - credential status upsert now re-resolves inserted status IDs when needed.
- Updated provider usage-recording flow:
  - stream usage fallback now parses OpenAI Responses SSE frames for `usage`
  - request payload bytes are retained for token estimation when upstream usage is unavailable.
- Updated release workflow to publish signed checksum artifacts (`*.zip.sha256.sig`) in release/staging assets and generated S3 manifests.

### Fixed

- Fixed OpenAI Responses compatibility for reasoning/message item serialization:
  - `ResponseReasoningItem.id` is now optional
  - empty reasoning summaries (`summary: []`) are preserved during serialization
  - empty `ResponseOutputMessage.id` is now omitted instead of serialized as an empty string.
- Fixed provider-prefix extraction/model detection robustness by switching to JSON payload pointer extraction across operation/protocol combinations.

### Docs

- Updated EN/ZH `getting-started` examples:
  - use unscoped `GET /v1/models` as the minimal verification path
  - added scoped channel example (`/claudecode/v1/models`).

### Compatibility

- `global.update_source` should now use `github` or `web-hosted`.
  - legacy values such as `international` / `china` are normalized to `github`.
- Antigravity project-scoped forward requests now consistently carry a deterministic session identifier (`request.sessionId` and `x-machine-session-id`).
- Self-update verification now expects signed checksum metadata (`.sha256` + `.sha256.sig`) in release assets/manifests.
- Admin upsert clients should read returned `id` for newly created providers/credentials/users.

## v0.3.21

### Added

- Added graceful shutdown handling in app runtime:
  - server now listens for `Ctrl+C` and `SIGTERM`
  - `axum::serve` now exits via `with_graceful_shutdown(...)` instead of abrupt process termination.
- Added cross-log trace correlation for provider traffic:
  - downstream ingress `trace_id` is propagated internally and persisted as `downstream_trace_id` in upstream request logs and usage logs
  - admin/user usage and request tables now prefer correlated downstream trace ids when available.

### Changed

- Updated cache-affinity TTL parsing for Claude explicit breakpoints:
  - explicit `ttl: "5m"` now correctly maps to `5m` affinity TTL
  - explicit breakpoints without `ttl` continue to use `1h`.
- Updated admin request log UX:
  - added a body-capture hint banner explaining that request/response body persistence requires disabling `mask_sensitive_info` in Global Settings.
- Updated locale switcher labels and cache-breakpoint editor styles:
  - language toggle labels were normalized (`中文/EN`, `中/EN`)
  - cache breakpoint panel/cards/slots now use theme-aware styles for better dark-mode readability.
- Updated deployment docs/instructions:
  - Docker quickstart now uses prebuilt image pull guidance and explicit `GPROXY_HOST=0.0.0.0` for container exposure.

### Compatibility

- Default host in config/examples is now `127.0.0.1` for local-safe defaults.
- For containerized/public binding, continue to set `GPROXY_HOST=0.0.0.0`.

## v0.3.20

### Changed

- Updated Claude / ClaudeCode cache-control guidance to the current `cache_breakpoints` model:
  - removed old docs/examples based on `enable_top_level_cache_control`
  - documented rule shape (`target`, `position`, `index`, `ttl`) and 4-slot limit semantics.
- Clarified no-ttl default behavior for Claude-family channels:
  - `claudecode` without `ttl` defaults to `1h`
  - `claude` without `ttl` defaults to `5m`.
- Documented Anthropic TTL ordering constraint for mixed `5m` and `1h` breakpoints (`tools -> system -> messages`).
- Updated magic-trigger cache insertion to enforce the same 4-breakpoint global cap:
  - `existing cache_control count + magic-trigger insertions <= 4`
  - when budget is exhausted, trigger strings are still removed but no new cache_control is injected.
  - supported magic trigger strings:
    - `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH auto`
    - `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF 5m`
    - `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY 1h`
- Updated system update channel selection:
  - `/admin/system/latest_release` and `/admin/system/self_update` now accept optional `update_channel` query (`releases` / `staging`)
  - Admin Global Settings adds a frontend-only `update_channel` selector and passes it via request query
  - no new backend global-settings field is introduced for update channel persistence.
- Updated SeaORM 2 schema sync behavior in storage initialization:
  - removed SQLite-specific handwritten runtime `ALTER TABLE` fallback for `downstream_trace_id`
  - now relies on SeaORM 2 `schema.sync()` entity diff to auto-add missing columns on existing tables.

### Docs

- Reworked EN/ZH docs pages:
  - `guides/configuration`
  - `guides/credential-selection-cache-affinity`
  to reflect cache breakpoint rewrite, magic-trigger behavior, and channel-specific no-ttl defaults.
- Updated `README.md` / `README.zh.md` cache sections and quick-check instructions to use `cache_breakpoints`.
- Updated `gproxy.example.full.toml` Claude / ClaudeCode examples from legacy top-level flag to `cache_breakpoints`.

## v0.3.19

### Added

- Added `append_query_param_if_missing` utility in provider channel helpers, with unit tests to:
  - append a query key/value when missing
  - keep existing query intact when key already exists
  - avoid duplicated query keys.

### Changed

- Updated Admin `Request Log` payload query handling:
  - `req query` now normalizes leading `?` and empty values
  - upstream rows now derive `req query` from `request_url` when downstream-style `request_query` is unavailable
  - payload view now consistently shows `req query` (or `-` when empty).
- Updated Claude and ClaudeCode upstream request path building:
  - requests now default to include `beta=true` in query string (without duplicating an existing `beta` key).

### Compatibility

- No breaking API shape changes.
- If an upstream request path already includes `beta=...`, GProxy preserves existing `beta` and does not append another one.

## v0.3.18

### Changed

- Updated binary release workflow S3 auth handling:
  - removed `aws-actions/configure-aws-credentials`
  - upload/manifest steps now use explicit AWS env vars (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_DEFAULT_REGION`) for better S3-compatible endpoint behavior.
- Updated release-channel naming from `release` to `releases` across binary publish/update flow:
  - workflow now generates/uploads `releases/manifest.json`
  - system update default channel now resolves to `releases`.
- Updated docs downloads manifest URL resolution:
  - when base URL ends with `/downloads`, frontend now resolves manifest from site root (`/manifest.json`)
  - load status/error now includes the concrete manifest URL for easier troubleshooting.
- Updated docs downloads page interaction and layout:
  - `releases/` and `staging/` are presented as navigable directory entries.
  - mobile now keeps the same tabular layout as desktop with horizontal scrolling.
  - folder rows use the same plain text style as file rows (no extra visual effect).

### Fixed

- Fixed duplicate upstream request records for tracked internal HTTP events:
  - internal tracked events now receive primary upstream request metadata and skip writing duplicated primary-request entries
  - applied for both success and error paths in provider execution/OAuth/usage handlers.

### Compatibility

- If you previously consumed `release/manifest.json`, switch to `releases/manifest.json`.

## v0.3.17

### Added

- Added S3-compatible binary publish flow in `release-binary.yml` for both:
  - `push` -> upload staging artifacts to `staging/*`
  - `release` -> upload latest artifacts to root and versioned artifacts to `releases/<version>/*`.
- Added manifest generation from bucket object listing:
  - global `manifest.json` for downloads page
  - channel manifests `release/manifest.json` and `staging/manifest.json` for updater compatibility.
- Added cache-affinity trace logging controlled by `GPROXY_AFFINITY_TRACE` to inspect key hit/miss, credential pick, bind/clear, and retry flow.
- Added reusable docs downloads component with EN/ZH pages and language switch.

### Changed

- Updated cache-affinity selection scoring:
  - each affinity candidate now carries `key_len`
  - `RoundRobinWithCache` picks credential by summed `key_len` over the contiguous hit prefix.
- Updated affinity scan behavior to stop at first miss, or first hit mapped to an ineligible credential.
- Updated Claude/ClaudeCode top-level `cache_control: {"type":"ephemeral"}` default affinity TTL from `5m` to `1h`.
- Updated docs pipeline:
  - docs deploy now runs on both default-branch `push` and `release` events
  - downloads content now reads a bucket-generated manifest (replacing the docs-side sync script flow).
- Updated China update-source default base URL to `https://download-gproxy.leenhawk.com`.

### Fixed

- Fixed affinity block hashing so changing a middle block does not cascade hash changes to later blocks.
- Fixed retry cleanup behavior for cache affinity to clear only the matched key for the failed attempt.

### Compatibility

- Removed legacy cache-affinity compatibility paths:
  - `cache_affinity_enabled` is no longer parsed as an effective mode override
  - `credential_pick_mode = sticky_with_cache` is no longer accepted as a compatibility mode.
- When legacy cache-affinity fields are present, effective behavior falls back to `RoundRobinWithCache`.

## v0.3.16

### Changed

- Updated Admin `Request Log` table columns:
  - upstream view now shows `credential_id`
  - downstream view keeps `credential_id` hidden (no extra column).
- Updated Admin `Usage` detail rows table to show `credential_id` for each usage row.

### Compatibility

- No API/protocol behavior changes in this release; updates are frontend display-only.

## v0.3.15

### Added

- Added docs `/downloads` index page that renders `release` and `staging` asset lists from manifests.
- Added admin console startup release check (`/admin/system/latest_release`) with auto-dismiss toast notification when a newer release is available.
- Added copy-to-clipboard buttons in Admin `Request Log` payload view: `req body` / `resp body` now have a clipboard icon next to the eye toggle.

## v0.3.14

### Added

- Added OpenAI Chat Completions `reasoning_details` support across request/stream/types, including stream<->non-stream aggregation paths.
- Added Claude thinking-to-OpenAI chat streaming mapping for both `reasoning_content` and encrypted `reasoning_details` (`reasoning.encrypted`).
- Added ClaudeCode model-list alias expansion: `/v1/models` now includes paired `-nothinking` model IDs.
- Added protocol-aware transform stream serialization error chunks:
  - Gemini NDJSON emits one-line JSON error objects.
  - SSE protocols emit `event: error` chunks.
- Added Cloudflare publish secret validation in release workflow before deployment steps run.

### Changed

- Updated OpenAI->Claude reasoning defaults and budgeting:
  - when chat `reasoning_effort` is omitted, `claude-sonnet-4-6` / `claude-opus-4-6` default to `adaptive`, while other Claude models default to `disabled`.
  - medium effort maps to budgeted Claude thinking with max-token-aware clamping.
- Refined OpenAI chat/embeddings extra thinking config mapping in transform flows.
- ClaudeCode upstream request normalization supports `-nothinking` only for `claude-sonnet-4-6` / `claude-opus-4-6` (strip suffix + remove `thinking` before forwarding).

### Fixed

- Fixed transform streaming failure behavior: serialization issues now surface as stream error chunks instead of abrupt stream termination.
- Fixed Claude reasoning budget edge cases where computed budget could exceed allowed max-token bounds.
- Fixed Windows release workflow by excluding `aarch64-pc-windows-msvc` from UPX installation path.

### Compatibility

- Streaming clients should handle transform-time stream error payloads (`event: error` for SSE, JSON error lines for Gemini NDJSON).
- ClaudeCode model listing returns extra `-nothinking` IDs only for `claude-sonnet-4-6` / `claude-opus-4-6`.

## v0.3.13

### Added

- Added automatic upstream HTTP tracking in provider runtime via a unified tracked request wrapper (`wreq` path).
- Added internal upstream request event ingestion for auto-captured request/response metadata (`internal=true`), including OAuth/cookie/token/profile side calls.
- Added docs publish integration for Cloudflare Pages with latest release package sync for the `/downloads` page.

### Changed

- Switched provider-side direct HTTP calls to tracked request flow so OAuth and auxiliary upstream calls are recorded without manual per-branch wiring.
- Wrapped provider execution and OAuth/usage handlers with per-request tracked HTTP capture scopes and centralized event enqueueing.
- Increased body capture limits from `32MB` to `50MB` in provider/downstream capture paths.

### Fixed

- Fixed missing upstream records for OAuth-related upstream calls, including ClaudeCode cookie exchange/token refresh and related OAuth helper requests.
- Fixed gaps where non-main upstream requests (for example token/profile side calls) could bypass upstream recording.
- Fixed missing status code in auto-captured upstream events on failed requests.
- Fixed missing error response payload in upstream records by capturing upstream error bodies (when available).

### Compatibility

- Upstream request log volume can increase after upgrade because internal auxiliary upstream calls are now recorded.
- To inspect only user-facing/main upstream requests, filter with `internal = false` in admin queries.
- `mask_sensitive_info` remains enabled by default; when enabled, request/response bodies are masked in stored records.

## v0.3.12

### Changed

- Added request/usage counting capabilities and related admin APIs for upstream/downstream visibility.
- Added request path filtering in the admin request module.
- Added context flags for recording upstream and stream usage events.
- Enforced direct stream-to-stream chat conversion paths and completed direct chat mapping behavior.
- Added broader `reasoning_content` support in Chat Completions data structures and transform flows.
- Enhanced MCP tool-use handling in OpenAI Chat Completions streaming transforms.
- Split China update channels into separate `staging`/`release` feeds and added update source configuration.
- Improved downstream event handling with async-stream based processing and updated related dependencies.
- Added docs asset sync script (`docs/scripts/sync-downloads.mjs`) and updated docs/homepage links.
- Improved frontend usability and i18n copy (responsive table wrapper, navigation labels, provider mode labels, OAuth wording).

### Fixed

- Fixed token usage accounting across Claude/OpenAI/Gemini response transforms:
  - cache creation/read tokens are now consistently reflected in input/prompt/total token semantics
  - applied to both non-stream and stream transforms, including reverse-direction mappings.
- Fixed stream mapping gaps by removing unsupported Gemini metadata fallback paths in direct chat stream transforms.

### Compatibility

- Usage values (especially input/prompt/total) can be higher than `v0.3.11` in cache-heavy traffic due to corrected accounting semantics.
- If you use usage numbers for billing, quota, or alerts, recalibrate thresholds after upgrading.

## v0.3.11

### Changed

- Reworked admin provider workspace into four tabs: `Single Add`, `Bulk Import/Export`, `Credential List`, and `Config`.
- Moved OAuth flows under `Single Add`; credential cards now live in `Credential List`, and editing opens inline below the list.
- Added credential auto-naming fallback for create/import and card display: prefer `user_email`, then fall back to key/cookie prefix when name is empty.
- Admin default landing module is now `Providers` (instead of `Global Settings`).
- Added provider/credential list search with mode switch (`By ID` / `By Name`).
- Added pagination plus page-size selection (`5/10/20/50`) for providers, credentials, request logs, usage rows, users, and user keys.
- Added responsive default page size by viewport (mobile/tablet/desktop/large desktop).
- Updated topbar and app-shell UX:
  - show app version and short commit hash
  - locale switch is now a single-toggle segmented control (`CN/EN` or `中/英`, based on locale)
  - light/dark switch moved to a draggable floating action button.
- Improved mobile navigation UX:
  - sidebar can collapse behind a hamburger toggle
  - toggle animates to `X` on expand
  - active nav entry is hidden in expanded list to avoid duplicate current-item display.

### Fixed

- Fixed Android self-update asset resolution by supporting `gproxy-android-<arch>.zip`.
- Fixed custom provider ID allocation to avoid collisions with builtin provider IDs by moving custom IDs to the `>= 1000` range across frontend creation, bootstrap seeding, and config import.

### Compatibility

- Custom provider IDs below `1000` are now rejected by admin upsert API for custom channels.
- Admin hash-route fallback now resolves to `#/admin/providers` when module is missing or invalid.

## v0.3.10

### Changed

- Added top-level cache-control mode enum for Claude/ClaudeCode provider settings and admin UI:
  - `off` / `auto` / `5m` / `1h`.
- Updated top-level cache-control injection behavior:
  - `auto` injects `{"type":"ephemeral"}` without TTL
  - `5m` / `1h` inject `{"type":"ephemeral","ttl":"5m|1h"}`.
- Enhanced provider usage metrics and frontend display:
  - added cache read input tokens
  - added cache creation input tokens
  - added cache creation split by TTL (`5m`, `1h`).
- Updated EN/ZH i18n entries for cache-control modes and usage token labels.

### Fixed

- Fixed user-key copy/visibility button clickability in admin/user key lists.
- Added copy success/failure feedback for admin-side user-key copy action.
- Fixed Claude cache-affinity fallback for top-level cache-control without TTL:
  - default fallback is now `5m` (was `1h`).

### Compatibility

- `enable_top_level_cache_control` no longer accepts legacy boolean values (`true` / `false`).
- Use string mode values only: `off`, `auto`, `5m`, `1h`.

## v0.3.9

### Changed

- Add unquote and normalizeClaudeCodeCookie functions for credential handling

## v0.3.8

### Changed

- Updated workspace/package release version to `0.3.8`.
- Updated CI release workflow:
  - `push` on `main/master` now publishes a `staging` prerelease (not marked as latest)
  - `push` on `main/master` now also publishes Docker staging tags (`staging`, `staging-<sha>`, and `-musl` variants)
  - stable `release` publish behavior remains unchanged (`latest` + tag-based images).
- Refactored cache-affinity key derivation for content-generation routes:
  - switched from whole-body/single-key affinity to protocol-aware prefix-block affinity with multi-candidate matching
  - kept in-memory affinity pool storage/key scoping in v1 format (`{channel}::{affinity_key}`)
  - aligned key derivation across OpenAI Chat/Responses, Claude Messages, and Gemini GenerateContent paths.
- Unified affinity hint calculation callsites across upstream channels:
  - affinity hint now derives from `protocol + prepared model + prepared body`
  - avoids protocol ambiguity and keeps hint calculation consistent with final outbound request payload.
- Updated Claude/ClaudeCode top-level cache-control injection behavior:
  - when enabled and absent in request, injects top-level `{"type":"ephemeral"}` only
  - no longer injects client-side TTL; effective TTL is left to upstream server semantics.
- Add daily update schedule for npm packages in /docs directory

### Fixed

- Fixed self-update channel routing:
  - staging builds now track the `staging` release stream
  - stable builds continue to track the latest stable release stream.
- Fixed Claude auto cache-affinity TTL classification:
  - top-level `cache_control: {"type":"ephemeral"}` now maps to `1h` affinity TTL to match upstream automatic caching behavior
  - explicit/other Claude cache-control TTL handling remains unchanged (`1h` when declared, otherwise `5m`).
- Fixed retry-affinity lifecycle behavior:
  - on retry after affinity-hit failure, only the matched key mapping is cleared
  - on success, bind key is always written and matched key TTL is refreshed.
- Update ANTIGRAVITY_USER_AGENT version and modify request handling for Gemini model

### Docs

- Updated cache-affinity design docs (EN/ZH):
  - clarified credential pick modes and internal affinity pool behavior
  - documented v1 key derivation/TTL rules for OpenAI, Claude, and Gemini
  - documented multi-candidate matching, bind-key behavior, and retry cleanup semantics.

## v0.3.7

### Changed

- Updated workspace/package release version to `0.3.7`.
- Reverted the temporary OAuth-specific provider fallback patch; provider availability now follows unified bootstrap seeding.

### Fixed

- Fixed startup provider bootstrap when `config.toml` is absent:
  - builtin providers are now loaded into in-memory registry during bootstrap seeding
  - builtin providers are persisted to storage with stable IDs at the same time
  - provider-dependent endpoints (including OAuth and usage routes) no longer fail due to missing in-memory providers.

## v0.3.6

### Added

- Added a User-Agent template selector in admin provider config:
  - channel presets for `gproxy`, `codex`, `claudecode`, `geminicli`, `antigravity`
  - classic IDE presets (`VS Code`, `IntelliJ IDEA`, `PyCharm`)
  - classic bot presets (`Googlebot`, `Bingbot`).

### Fixed

- Fixed frontend default gproxy User-Agent draft from placeholder `os,arch` to build-time resolved values.

## v0.3.5

### Changed

- Updated workspace/package release version to `0.3.5`.
- Added stable default IDs for builtin channels during bootstrap seeding.
- Updated login UX defaults: username prefilled as `admin`, password hinted from startup logs.

### Fixed

- Fixed OAuth start/callback provider resolution when provider config exists in storage but is not yet loaded in memory:
  - fallback lookup now loads enabled provider config from storage
  - resolved provider config is cached back into in-memory state for subsequent OAuth requests.

## v0.3.4

### Fixed

- Fixed default SQLite DSN path for container deployments by switching from `sqlite://app/data/gproxy.db?mode=rwc` to `sqlite:///app/data/gproxy.db?mode=rwc`.
- Fixed Zeabur startup failure (`code: 14 unable to open database file`) caused by the incorrect default DSN path.
- Updated deployment docs and examples (`README`, `README.zh`, docs deployment guides, `zeabur.yaml`) to use the corrected DSN format.
- Update Cargo.toml to use workspace settings for version and edition

## v0.3.3

### Added

- Added per-channel `user_agent` settings across builtin/custom providers in admin config and backend settings schemas.
- Added global `spoof_emulation` setting with admin UI, persistence, and runtime HTTP client wiring.
- Enhanced credential management UX:
  - added OAuth as a dedicated credentials sub-tab
  - added quick single-credential add (raw key or JSON payload)
  - added clipboard copy actions for user keys and credential cards.
- Added automatic provider credential refresh after successful single add, OAuth completion, and batch import.

### Changed

- Refactored provider settings parsing architecture:
  - removed legacy monolithic parser approach
  - moved JSON patch parsing into each channel `settings.rs` via `from_provider_settings_value`
  - kept top-level parser focused on channel `match` dispatch.
- Improved upstream `user_agent` resolution and normalization flow, while preserving channel-specific default UA behaviors.
- ClaudeCode OAuth refresh flow now backfills missing account metadata (for example subscription/rate-limit profile fields) from profile endpoints.

### Fixed

- Improved self-update reliability for GitHub release fetch/download:
  - added proxy/direct client fallback flow for update requests
  - direct self-update client now explicitly disables inherited system proxy (`no_proxy`) to reduce `ProxyConnect` failures.
- Fixed credential copy payload behavior:
  - key-based channels now return/copy plain key value
  - JSON-based channels now return/copy normalized `secret_json`.
- Fixed explicit empty `user_agent` handling so empty UA can be intentionally configured and forwarded.
- Reduced default log noise by suppressing SQL statement-level logs (`sqlx` / `sea_orm` default to `warn` unless `RUST_LOG` overrides).

## v0.3.2

### Changed

- Improved model route protocol selection (`/v1/models`, `/{provider}/v1/models`, and model get routes):
  - prioritize Gemini model flow when `x-goog-api-key` or query `?key=` is provided
  - treat `anthropic-version` as Claude preference only when `Authorization: Bearer ...` exists
  - keep OpenAI as default fallback when no explicit Gemini/Claude preference is detected
- Updated request log UI to expose richer request context with payload drill-down (headers/body preview) for faster debugging.
- Added additional request normalization for Codex channel:
  - normalize model ids by stripping `codex/` prefix when needed
  - convert `system`/`developer` input messages into `instructions` before forwarding to upstream `/responses`

### Fixed

- Fixed local transformed responses for model endpoints not being unwrapped correctly:
  - removed enum wrapper shells like `ModelListOpenAi`
  - removed HTTP wrapper envelope (`stats_code/headers/body`) and returned normalized body payload directly
- Fixed Codex chat completion replay failures caused by upstream rejecting `system` messages (`System messages are not allowed`).
- Fixed GeminiCli long/stream chat completion replay compatibility by stripping unsupported `generationConfig.logprobs` and `generationConfig.responseLogprobs`, avoiding upstream `400` empty-body responses in this scenario.

## v0.3.1

### Changed

- Updated the login view to accept username and password instead of API key.
- Modified the API request to handle username and password for user authentication.
- Added password field to user-related data structures and storage.
- Implemented user key generation upon successful login if no existing key is found.
- Updated the Chinese localization files to reflect changes in the login process.
- Refactored user management to accommodate password handling in user creation and updates.


## v0.3.0

### Added
- Built-in `groq` channel support, including admin-side channel schema and provider execution wiring.
- Admin credential status management APIs (`query/upsert/delete`) and corresponding runtime status handling.
- Zeabur deployment template (`zeabur.yaml`) for one-click cloud deployment.
- Full docs site (Starlight) with bilingual content (`en` default + `zh` locale).

### Changed
- Refactored provider/channel registry flow to consolidate builtin/custom channel metadata and credential creation logic.
- Improved request routing and recording with stronger provider-model prefix parsing for unscoped routes.
- Refactored admin frontend provider/credential modules for clearer channel-specific settings/dispatch handling.
- Updated release/build pipeline and Docker build flow for more stable multi-architecture outputs.

### Improved
- Admin and user usage/request filter experience in frontend, including better searchable filter options.
- i18n message organization in frontend by splitting language messages into dedicated modules.
- Docs structure now includes dedicated deployment guide sections:
  - local deployment: binary + Docker
  - cloud deployment: Zeabur

### Fixed
- Favicon/static asset serving behavior in frontend/docs entry pages.
- Multiple provider/frontend consistency issues in channel settings, dispatch mapping, and filter option loading.
