# gproxy

gproxy is a Rust-based proxy service for multiple AI providers with credential management,
OAuth helpers, and an optional embedded web control panel.

## Features
- Multi-provider routing (OpenAI, Claude, Claude Code, Codex, Gemini CLI, Antigravity, Vertex, etc.)
- Admin API for configs and credentials
- Usage tracking and per-credential usage views
- Optional embedded web UI (React + Tailwind via CDN)

## Quick start
1. Copy config:
   - `cp gproxy.example.toml gproxy.toml`
2. Run:
   - `cargo run --release`
3. Open admin UI (if `front` feature enabled):
   - `http://127.0.0.1:8787/`

## Admin key
The admin key is stored in `gproxy.toml` under `app.admin_key`. The web UI uses it to
authenticate management actions. API requests accept it via:
- `x-admin-key` (admin routes)
- provider usage routes:
  - Codex: `Authorization: Bearer <admin_key>`
  - Claude Code: `x-api-key` + `anthropic-version`
  - Antigravity: `x-goog-api-key`

## Web UI
The embedded UI is served from `front/dist` when the `front` feature is enabled (default).
It supports:
- Channel tabs with upload + management sub-tabs
- OAuth helper flow for Gemini CLI / Antigravity / Codex / Claude Code
- Usage view per credential and live usage where available
- Batch upload for selected providers
- Base URL editing (only `base_url` is exposed)

## Build notes
- Default features include `front`. To disable the UI:
  - `cargo build --no-default-features --features storage,providers`

## Docker
Build:
- `docker build -t gproxy:local .`

Run (mount config + data):
- `docker run --rm -p 8787:8787 -v $(pwd)/gproxy.toml:/app/gproxy.toml -v $(pwd)/data:/app/data gproxy:local`

Notes:
- Default config path is `./gproxy.toml` (relative to working directory).
- File storage uses `./data` for usage views by default.

## Environment variables
General:
- `RUST_LOG`: tracing filter (`info`, `debug`, etc.).
- `GPROXY_DEBUG_HTTP`: `1`/`true` logs upstream request/response headers (auth headers are stripped).

Storage selection:
- `GPROXY_STORAGE`: choose where config/credentials live (`memory`, `file`, `database`, `s3`). Default is `file`.

File storage (`GPROXY_STORAGE=file`):
- `GPROXY_STORAGE_FILE_PATH`: config file path (default `./gproxy.toml`).
- `GPROXY_STORAGE_FILE_DATA_DIR`: usage data directory (default `./data`).
- `GPROXY_STORAGE_FILE_DEBOUNCE_SECS`: write debounce in seconds (0 = write immediately).

Database storage (`GPROXY_STORAGE=database`):
- `GPROXY_STORAGE_DB_URI`: database URL (e.g. `sqlite://gproxy.db`, `postgres://...`).
- `GPROXY_STORAGE_DB_MAX_CONNECTIONS`: optional pool max.
- `GPROXY_STORAGE_DB_MIN_CONNECTIONS`: optional pool min.
- `GPROXY_STORAGE_DB_CONNECT_TIMEOUT_SECS`: optional connect timeout.
- `GPROXY_STORAGE_DB_SCHEMA`: optional schema name.
- `GPROXY_STORAGE_DB_SSL_MODE`: optional SSL mode string.
- `GPROXY_STORAGE_DB_DEBOUNCE_SECS`: write debounce in seconds.

S3 storage (`GPROXY_STORAGE=s3`):
- `GPROXY_STORAGE_S3_BUCKET`: bucket name.
- `GPROXY_STORAGE_S3_REGION`: bucket region.
- `GPROXY_STORAGE_S3_ACCESS_KEY`: access key.
- `GPROXY_STORAGE_S3_SECRET_KEY`: secret key.
- `GPROXY_STORAGE_S3_ENDPOINT`: custom endpoint (S3-compatible).
- `GPROXY_STORAGE_S3_PATH_STYLE`: `true` for path-style addressing.
- `GPROXY_STORAGE_S3_SESSION_TOKEN`: optional session token.
- `GPROXY_STORAGE_S3_USE_TLS`: optional TLS toggle.
- `GPROXY_STORAGE_S3_PATH`: config object key (default `gproxy.toml`).
- `GPROXY_STORAGE_S3_DEBOUNCE_SECS`: write debounce in seconds.

Note for S3/DB: if the remote store is empty and a local `gproxy.toml` exists, it is used to seed the remote config on first run.

## CLI arguments
CLI flags mirror the env vars above and override them when both are set:
- `--storage <memory|file|database|s3>`: select storage backend.
- `--storage-file-path <path>`: config file path for file storage.
- `--storage-file-data-dir <path>`: usage data directory for file storage.
- `--storage-file-debounce-secs <seconds>`: write debounce for file storage.
- `--storage-db-uri <url>`: database URL for database storage.
- `--storage-db-max-connections <n>`: max DB connections.
- `--storage-db-min-connections <n>`: min DB connections.
- `--storage-db-connect-timeout-secs <seconds>`: DB connect timeout.
- `--storage-db-schema <name>`: DB schema name.
- `--storage-db-ssl-mode <value>`: DB SSL mode.
- `--storage-db-debounce-secs <seconds>`: write debounce for DB storage.
- `--storage-s3-bucket <name>`: S3 bucket.
- `--storage-s3-region <region>`: S3 region.
- `--storage-s3-access-key <key>`: S3 access key.
- `--storage-s3-secret-key <key>`: S3 secret key.
- `--storage-s3-endpoint <url>`: S3-compatible endpoint.
- `--storage-s3-path-style <true|false>`: path-style addressing toggle.
- `--storage-s3-session-token <token>`: S3 session token.
- `--storage-s3-use-tls <true|false>`: TLS toggle.
- `--storage-s3-path <path>`: config object key for S3.
- `--storage-s3-debounce-secs <seconds>`: write debounce for S3 storage.

## Routes
Web UI (when `front` feature enabled):
- `GET /`
- `GET /index.html`
- `GET /{*path}` (static assets)

Admin API (require `x-admin-key`):
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
- `POST /admin/geminicli/fetch-email` (when Gemini CLI enabled)
- `POST /admin/antigravity/fetch-email` (when Antigravity enabled)

Provider OAuth and usage:
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

Provider proxy routes (all providers):
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

Additional OpenAI responses (OpenAI provider only):
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

Additional Claude endpoints (Claude provider only):
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

## License
AGPL-3.0-or-later

## Thank

- [clewdr](https://github.com/Xerxes-2/clewdr)
- [gcli2api](https://github.com/su-kaka/gcli2api)