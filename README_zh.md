# gproxy

gproxy 是一个基于 Rust 的多渠道代理服务，提供凭证管理、OAuth 辅助和可选的内嵌控制面板。

## 功能
- 多渠道路由（OpenAI、Claude、Claude Code、Codex、Gemini CLI、Antigravity、Vertex 等）
- 管理端 API（配置与凭证）
- 用量统计与凭证级 usage view
- 可选内嵌 Web 控制面板（React + Tailwind，CDN 方式）

## 快速开始
1. 复制配置：
   - `cp gproxy.example.toml gproxy.toml`
2. 运行：
   - `cargo run --release`
3. 打开管理界面（启用 `front` 特性时）：
   - `http://127.0.0.1:8787/`

## 管理密钥
管理密钥在 `gproxy.toml` 的 `app.admin_key`。前端会使用它进行管理操作认证。
API 使用方式：
- 管理端：`x-admin-key`
- usage 路由：
  - Codex：`Authorization: Bearer <admin_key>`
  - Claude Code：`x-api-key` + `anthropic-version`
  - Antigravity：`x-goog-api-key`

## 前端控制面板
启用 `front` 特性时会从 `front/dist` 提供内嵌 UI，包含：
- 渠道页 + 子标签（上传/管理）
- Gemini CLI / Antigravity / Codex / Claude Code 的 OAuth 辅助流程
- 凭证用量视图与可用的实时 usage
- 批量上传（部分渠道）
- 仅允许修改 `base_url`

## 构建说明
- 默认包含 `front` 特性。禁用 UI：
  - `cargo build --no-default-features --features storage,providers`

## Docker
构建：
- `docker build -t gproxy:local .`

运行（挂载配置与数据）：
- `docker run --rm -p 8787:8787 -v $(pwd)/gproxy.toml:/app/gproxy.toml -v $(pwd)/data:/app/data gproxy:local`

说明：
- 默认配置路径为 `./gproxy.toml`（相对工作目录）。
- 文件存储模式默认使用 `./data` 保存 usage view。

## 环境变量
通用：
- `RUST_LOG`：日志过滤器（如 `info`、`debug`）。
- `GPROXY_DEBUG_HTTP`：`1`/`true` 打开上游 HTTP 日志（已剔除鉴权头）。

存储选择：
- `GPROXY_STORAGE`：选择配置/凭证存放位置（`memory`、`file`、`database`、`s3`），默认 `file`。

文件存储（`GPROXY_STORAGE=file`）：
- `GPROXY_STORAGE_FILE_PATH`：配置文件路径（默认 `./gproxy.toml`）。
- `GPROXY_STORAGE_FILE_DATA_DIR`：usage 数据目录（默认 `./data`）。
- `GPROXY_STORAGE_FILE_DEBOUNCE_SECS`：写入防抖秒数（0 表示立即写入）。

数据库存储（`GPROXY_STORAGE=database`）：
- `GPROXY_STORAGE_DB_URI`：数据库连接地址（如 `sqlite://gproxy.db`、`postgres://...`）。
- `GPROXY_STORAGE_DB_MAX_CONNECTIONS`：连接池最大连接数（可选）。
- `GPROXY_STORAGE_DB_MIN_CONNECTIONS`：连接池最小连接数（可选）。
- `GPROXY_STORAGE_DB_CONNECT_TIMEOUT_SECS`：连接超时秒数（可选）。
- `GPROXY_STORAGE_DB_SCHEMA`：Schema 名称（可选）。
- `GPROXY_STORAGE_DB_SSL_MODE`：SSL 模式（可选）。
- `GPROXY_STORAGE_DB_DEBOUNCE_SECS`：写入防抖秒数。

S3 存储（`GPROXY_STORAGE=s3`）：
- `GPROXY_STORAGE_S3_BUCKET`：桶名。
- `GPROXY_STORAGE_S3_REGION`：区域。
- `GPROXY_STORAGE_S3_ACCESS_KEY`：访问密钥。
- `GPROXY_STORAGE_S3_SECRET_KEY`：访问密钥。
- `GPROXY_STORAGE_S3_ENDPOINT`：S3 兼容端点（可选）。
- `GPROXY_STORAGE_S3_PATH_STYLE`：路径风格地址（`true`/`false`）。
- `GPROXY_STORAGE_S3_SESSION_TOKEN`：会话令牌（可选）。
- `GPROXY_STORAGE_S3_USE_TLS`：是否使用 TLS（可选）。
- `GPROXY_STORAGE_S3_PATH`：配置对象 key（默认 `gproxy.toml`）。
- `GPROXY_STORAGE_S3_DEBOUNCE_SECS`：写入防抖秒数。

S3/DB 说明：远端为空且本地存在 `gproxy.toml` 时，会在首次启动写入远端作为初始配置。

## 命令行参数
命令行与环境变量语义一致，若同时设置则命令行优先：
- `--storage <memory|file|database|s3>`：选择存储后端。
- `--storage-file-path <path>`：文件存储配置路径。
- `--storage-file-data-dir <path>`：文件存储 usage 数据目录。
- `--storage-file-debounce-secs <seconds>`：文件存储写入防抖。
- `--storage-db-uri <url>`：数据库连接地址。
- `--storage-db-max-connections <n>`：数据库最大连接数。
- `--storage-db-min-connections <n>`：数据库最小连接数。
- `--storage-db-connect-timeout-secs <seconds>`：数据库连接超时。
- `--storage-db-schema <name>`：数据库 schema 名称。
- `--storage-db-ssl-mode <value>`：数据库 SSL 模式。
- `--storage-db-debounce-secs <seconds>`：数据库写入防抖。
- `--storage-s3-bucket <name>`：S3 桶名。
- `--storage-s3-region <region>`：S3 区域。
- `--storage-s3-access-key <key>`：S3 访问密钥。
- `--storage-s3-secret-key <key>`：S3 访问密钥。
- `--storage-s3-endpoint <url>`：S3 兼容端点。
- `--storage-s3-path-style <true|false>`：路径风格地址开关。
- `--storage-s3-session-token <token>`：S3 会话令牌。
- `--storage-s3-use-tls <true|false>`：TLS 开关。
- `--storage-s3-path <path>`：S3 配置对象 key。
- `--storage-s3-debounce-secs <seconds>`：S3 写入防抖。

## 路由
Web UI（启用 `front` 特性时）：
- `GET /`
- `GET /index.html`
- `GET /{*path}`（静态资源）

管理端 API（需要 `x-admin-key`）：
- `GET /admin/config`
- `PUT /admin/config`
- `GET /admin/config/export`
- `POST /admin/config/import`
- `GET /admin/providers/{name}/config`
- `PUT /admin/providers/{name}/config`
- `GET /admin/providers/{name}/credentials`
- `POST /admin/providers/{name}/credentials`
- `PUT /admin/providers/{name}/credentials/{index}`
- `DELETE /admin/providers/{name}/credentials/{index}`
- `GET /admin/usage?api_key=...`
- `GET /admin/usage/provider/{name}?credential_id=...`
- `POST /admin/geminicli/fetch-email`（启用 Gemini CLI 时）
- `POST /admin/antigravity/fetch-email`（启用 Antigravity 时）

OAuth 与 usage：
- `GET /geminicli/oauth`
- `GET /geminicli/oauth/callback`
- `GET /antigravity/oauth`
- `GET /antigravity/oauth/callback`
- `GET /antigravity/usage`
- `GET /codex/oauth`
- `GET /codex/oauth/callback`
- `GET /codex/usage`
- `GET /claudecode/oauth`
- `GET /claudecode/oauth/callback`
- `GET /claudecode/usage`

通用代理路由（所有 provider）：
- `POST /{provider}/v1/chat/completions`
- `POST /{provider}/v1/responses`
- `POST /{provider}/v1/responses/input_tokens`
- `POST /{provider}/v1/messages`
- `POST /{provider}/v1/messages/count_tokens`
- `GET /{provider}/v1beta/models`
- `GET /{provider}/v1beta/models/{name}`
- `POST /{provider}/v1beta/models/{name}`
- `GET /{provider}/v1/models`
- `GET /{provider}/v1/models/{model}`
- `POST /{provider}/v1/models/{model}`

OpenAI 扩展路由（仅 OpenAI provider）：
- `POST /openai/v1/conversations`
- `GET /openai/v1/conversations/{conversation_id}`
- `POST /openai/v1/conversations/{conversation_id}`
- `DELETE /openai/v1/conversations/{conversation_id}`
- `GET /openai/v1/conversations/{conversation_id}/items`
- `POST /openai/v1/conversations/{conversation_id}/items`
- `GET /openai/v1/conversations/{conversation_id}/items/{item_id}`
- `DELETE /openai/v1/conversations/{conversation_id}/items/{item_id}`
- `POST /openai/v1/responses/compact`
- `GET /openai/v1/responses/{response_id}`
- `DELETE /openai/v1/responses/{response_id}`
- `POST /openai/v1/responses/{response_id}/cancel`
- `GET /openai/v1/responses/{response_id}/input_items`

Claude 扩展路由（仅 Claude provider）：
- `POST /claude/v1/files`
- `GET /claude/v1/files`
- `GET /claude/v1/files/{file_id}`
- `DELETE /claude/v1/files/{file_id}`
- `GET /claude/v1/files/{file_id}/content`
- `POST /claude/v1/skills`
- `GET /claude/v1/skills`
- `GET /claude/v1/skills/{skill_id}`
- `DELETE /claude/v1/skills/{skill_id}`
- `POST /claude/v1/skills/{skill_id}/versions`
- `GET /claude/v1/skills/{skill_id}/versions`
- `GET /claude/v1/skills/{skill_id}/versions/{version}`
- `DELETE /claude/v1/skills/{skill_id}/versions/{version}`

## 许可证
AGPL-3.0-or-later

## 鸣谢

- [clewdr](https://github.com/Xerxes-2/clewdr)
- [gcli2api](https://github.com/su-kaka/gcli2api)