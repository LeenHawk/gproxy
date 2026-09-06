---
title: "Configuration"
description: "Flags, GPROXY_* environment variables, .env layering, native-only and build-time variables, and the instance settings stored in the database"
---

GPROXY reads its process configuration once, at startup, from command-line
flags, environment variables, and `.env` files. There is no other
configuration file: v3 does not read TOML. Everything that changes while
the process runs — providers, credentials, routes, rules, pricing,
identity, and the instance settings at the end of this page — lives in the
database and is edited through the console or the admin API.

`gproxy --help` is generated from the same declaration as the environment
list, so the two cannot drift. Every flag has a `GPROXY_*` twin; the tables
below name both.

## Precedence

A value is taken from the first source that sets it:

1. the command-line flag;
2. the process environment;
3. `./.env` in the working directory;
4. `<data-dir>/.env`, read only when it is a different file from `./.env`;
5. the built-in default.

`GPROXY_DATA_DIR` itself is resolved from the first three sources, because
the data directory must be known before its `.env` can be read. A relative
data directory is resolved against the working directory and created at
startup if it is missing.

## The `.env` Format

```bash
# <data-dir>/.env
GPROXY_HOST=0.0.0.0
GPROXY_PORT=8787
GPROXY_PERSISTENCE=postgres
GPROXY_DSN=postgres://gproxy:<password>@db.internal:5432/gproxy
GPROXY_MASTER_KEY=<standard-base64-32-bytes>
```

- One `KEY=value` per line. Keys and values are trimmed; quotes are not
  removed, so do not quote values.
- `#` starts a comment anywhere on a line, so a value cannot contain `#`.
  Put such a value in the real environment instead.
- A non-empty line without `=` is a startup error that names the file and
  line.
- Only keys that start with `GPROXY_`, plus `UPSTASH_URL` and
  `UPSTASH_TOKEN`, are read. Other keys in a shared deployment `.env` are
  ignored and never enter the process.

## Listen and Data

| Variable | Flag | Default | Meaning |
| --- | --- | --- | --- |
| `GPROXY_HOST` | `--host <ADDR>` | `127.0.0.1` | Interface to bind. `host:port` must parse as a socket address, so an IPv6 address needs brackets: `[::1]`. |
| `GPROXY_PORT` | `--port <PORT>` | `8787` | TCP port. |
| `GPROXY_DATA_DIR` | `--data-dir <PATH>` | `./data` | Holds the SQLite file `gproxy.db`, the optional `.env`, self-update staging (`.update/`), and the autostart marker. |

The container image presets `GPROXY_HOST=0.0.0.0`,
`GPROXY_DATA_DIR=/var/lib/gproxy` and `GPROXY_PERSISTENCE=sqlite`; see
[Container](/deployment/docker/).

## Persistence

| Variable | Flag | Default | Meaning |
| --- | --- | --- | --- |
| `GPROXY_PERSISTENCE` | `--persistence <BACKEND>` | `sqlite` | `sqlite`, `libsql`, `postgres`, or `mysql` (case-insensitive). |
| `GPROXY_DSN` | `--dsn <DSN>` | none | Connection string for `postgres` or `mysql`; required for those backends. |
| `GPROXY_LIBSQL_URL` | `--libsql-url <URL>` | none | Absolute `http(s)` URL of a libSQL server; required for `libsql`. |
| `GPROXY_LIBSQL_AUTH_TOKEN` | `--libsql-auth-token <TOKEN>` | none | Bearer token for that server; required and non-empty for `libsql`. |

DSN shapes and backend behaviour are on
[Storage & Cache Backends](/reference/database/).

## Cache

| Variable | Flag | Default | Meaning |
| --- | --- | --- | --- |
| `GPROXY_REDIS_URL` | `--redis-url <URL>` | none | Redis shared cache. Wins over Upstash when both are set. |
| `UPSTASH_URL` | `--upstash-url <URL>` | none | Upstash Redis REST endpoint; absolute `http(s)` URL. |
| `UPSTASH_TOKEN` | `--upstash-token <TOKEN>` | none | Upstash REST token. Setting only one of the two `UPSTASH_*` values is a startup error. |

With none of these set the cache is in-process, or a table in the libSQL
database when persistence is `libsql`. Quotas, rate limits, admission
state, refresh leases and affinity pins all live in the cache, so more
than one instance requires Redis or Upstash.

## Secrets at Rest

| Variable | Flag | Default | Meaning |
| --- | --- | --- | --- |
| `GPROXY_MASTER_KEY` | `--master-key <BASE64>` | unset | Standard base64 that decodes to exactly 32 bytes. When set, credentials, user API keys and the Hugging Face token are sealed with AES-256-GCM envelope encryption. Unset means plaintext storage. |
| `GPROXY_MASTER_KEY_NEXT` | `--master-key-next <BASE64>` | unset | Key to rotate to. An empty value rotates to plaintext. Ignored, with a warning, unless rotation is armed. |
| `GPROXY_MASTER_KEY_ROTATE` | `--master-key-rotate <BOOL>` | off | Arms the rotation: `1`, `true`, `yes`, `on`; or `0`, `false`, `no`, `off`, empty. Any other value is a startup error. |

The store records a SHA-256 fingerprint of the key it was sealed with.
Startup refuses a sealed store opened with a different or missing key, and
refuses a plaintext store opened with a key set. Moving in either
direction is a rotation, never a silent re-encryption.

Rotation procedure:

1. Leave `GPROXY_MASTER_KEY` at the current value (or unset for a plaintext
   store). Set `GPROXY_MASTER_KEY_NEXT` to the new key, or to an empty
   string to return to plaintext. Set `GPROXY_MASTER_KEY_ROTATE=on`.
2. Start GPROXY once. It opens every stored secret with the current key,
   re-seals it with the next key, and replaces the secret inventory and
   the fingerprint in one write. The log ends with a warning that tells
   you to finish the rotation.
3. Stop GPROXY. Set `GPROXY_MASTER_KEY` to the new key (or unset it), clear
   `GPROXY_MASTER_KEY_NEXT` and `GPROXY_MASTER_KEY_ROTATE`, and start again.

`GPROXY_MASTER_KEY_ROTATE=on` without `GPROXY_MASTER_KEY_NEXT` is a startup
error.

## Networking and Limits

| Variable | Flag | Default | Meaning |
| --- | --- | --- | --- |
| `GPROXY_UPSTREAM_PROXY_URL` | `--upstream-proxy-url <URL>` | none | Default outbound proxy. Precedence: credential proxy, then provider proxy, then this value. Overrides the `proxy` instance setting. Ambient `HTTP_PROXY`/`HTTPS_PROXY` are ignored unless `inherit_system_proxy` is on. Also used for update and announcement fetches. |
| `GPROXY_TRUSTED_PROXIES` | `--trusted-proxy <IP>` | empty | Comma-separated IPs. `X-Forwarded-For` (first entry) and `X-Real-IP` are honoured only from loopback or a listed peer. |
| `GPROXY_CORS_ORIGINS` | `--cors-origin <ORIGIN>` | empty | Comma-separated exact origins. Empty sends no CORS headers (same-origin only). Allowed methods `GET, POST, PATCH, DELETE, OPTIONS`; headers `authorization, content-type, x-api-key`; credentials allowed. |
| `GPROXY_MAX_ATTEMPTS` | `--max-attempts <COUNT>` | `6` | Upper bound on upstream attempts per request. A route's own `max_attempts` is capped by it. Must be positive. |
| `GPROXY_MAX_IN_FLIGHT` | `--max-in-flight <COUNT>` | `1024` | Concurrent requests the listener serves. Every request, including the console and admin API, takes one permit; further requests wait. Must be positive. |
| `GPROXY_FILE_UPLOAD_MAX_IN_FLIGHT` | `--file-upload-max-in-flight <COUNT>` | unset | Concurrent `POST /v1/files` and `POST /upload/v1beta/files` uploads per process. `0` is unlimited. When set it overrides the console setting of the same name. |
| `GPROXY_INSTANCE_ID` | `--instance-id <ID>` | `0` | Leading component of native request ids (`<instance>-<boot prefix>-<sequence>`). Give each instance in a fleet a distinct value. |
| `GPROXY_LOG_FORMAT` | `--log-format <FORMAT>` | `text` | `text` or `json` (newline-delimited). |
| `RUST_LOG` | — | `info` | Standard `tracing` filter for the native log. Read from the process environment only. |

Request bodies are limited to 100 MiB. `Content-Encoding: zstd` bodies are
decoded at ingress; other encodings are rejected with 415. Neither limit
is configurable.

## First-Run Bootstrap

These apply to a fresh store, one with no administrator yet. Without
`GPROXY_ADMIN_PASSWORD`, the first visit to `/admin` shows the setup
screen that creates the administrator.

| Variable | Flag | Default | Meaning |
| --- | --- | --- | --- |
| `GPROXY_ADMIN_USER` | `--admin-user <USER>` | `admin` | Administrator username used by bootstrap. |
| `GPROXY_ADMIN_PASSWORD` | `--admin-password <PASSWORD>` | unset | Fresh store: creates the administrator with this password and an API key. Existing store: resets the password of this user if it exists; other accounts are never touched. |
| `GPROXY_BOOTSTRAP_ADMIN_API_KEY` | `--bootstrap-admin-api-key <KEY>` | generated | Fresh store only, and only with `GPROXY_ADMIN_PASSWORD`: the administrator's first API key. Otherwise a random key is generated. Either way the key is sealed like any other and shown only through the console's reveal action. A blank value is an error. |
| `GPROXY_BOOTSTRAP_CHANNELS` | `--bootstrap-channel <CHANNEL>` | empty | Comma-separated channel ids. Fresh store only, with `GPROXY_ADMIN_PASSWORD`: creates one enabled provider per channel, named after it, with the channel's default rule set. An unknown id is a startup error. |

Setting a bootstrap key or channels on a fresh store without
`GPROXY_ADMIN_PASSWORD` is a startup error. A fresh store also loads the
embedded global price catalog; see [Pricing & Tiers](/reference/pricing/).

## Native Host Extras

The native binary reads these from the process environment only — not
from `.env` — and there are no flags for them.

| Variable | Default | Meaning |
| --- | --- | --- |
| `GPROXY_AUTOSTART` | `on` | First-run default of the per-user login entry (Linux `.desktop`, macOS LaunchAgent, Windows Run key). Read once, until `<data-dir>/.autostart-initialized` exists; afterwards the console's Login startup switch owns it. Accepts `on`/`off`, `true`/`false`, `1`/`0`, `yes`/`no`, `enable(d)`/`disable(d)`. The saved launch command repeats the current arguments and adds `--master-key` when `GPROXY_MASTER_KEY` was in the environment. |
| `GPROXY_UPDATE_CHANNEL_SERVE` | build channel | Update channel with the highest precedence: `releases` (also `release`, `stable`), `staging`, or `dev` (also `development`). |
| `GPROXY_UPDATE_CHANNEL` | build channel | Same values; consulted when `GPROXY_UPDATE_CHANNEL_SERVE` is unset. Full precedence: `_SERVE`, then `GPROXY_UPDATE_CHANNEL`, then the console's update channel setting, then the build channel. An invalid name makes update requests fail with 400. |
| `GPROXY_UPDATE_SERVE` | GitHub release URLs | Manifest URL override for every channel. Defaults: `dev` and `staging` read `releases/download/<channel>/manifest.json`, `releases` reads `releases/latest/download/manifest.json`, all from the GPROXY repository. |
| `GPROXY_UPDATE_RESTART` | `re-exec` | What happens after an applied update or rollback: `re-exec` (also `reexec`; execs the new binary with the same arguments on Unix, exit 42 elsewhere), `supervisor` (exit code 42 after 250 ms so a supervisor restarts it), or `none` (you restart). An invalid value disables self-update; its endpoints answer 503. |

An update is refused with 409 when the manifest's minimum data version is
above this binary's schema version, when the version is invalid, or when
no `<exe>.prev` exists for a rollback.

## Build-Time Identity

These are compile-time inputs read with `option_env!`, set in the
environment of `cargo build`. They are not runtime configuration.

| Variable | Default | Meaning |
| --- | --- | --- |
| `GPROXY_UPDATE_PUBKEY` | none | Standard base64 Ed25519 public key that verifies the signed update manifest. A build without it fails every update check with a signature error. |
| `GPROXY_BUILD_VERSION` | `CARGO_PKG_VERSION` | Version reported by `--version` and compared against manifests. |
| `GPROXY_BUILD_CHANNEL` | `development` | Update channel the build belongs to. `development` resolves to `dev`. |
| `GPROXY_BUILD_HASH` | git short hash | `build.rs` fills it from `git rev-parse --short=12 HEAD`; `unknown` without a repository. |
| `GPROXY_INSTALLATION_KIND` | `source` | Installation label reported by `--version`; installers set their own. |

```text
$ gproxy --version
gproxy 3.0.0 (channel development, build 4054fe4f94ea, installation source)
```

## Edge Bindings

The wasm host has no command line and reads no `.env`. The platform
wrapper passes bindings of the same names into the edge config:
`GPROXY_LIBSQL_URL` and `GPROXY_LIBSQL_AUTH_TOKEN` (required),
`GPROXY_MASTER_KEY`, `GPROXY_MASTER_KEY_NEXT` and `GPROXY_MASTER_KEY_ROTATE`
(optional), and `UPSTASH_URL` with `UPSTASH_TOKEN` (optional, together).
Persistence is always libSQL; the cache is the libSQL table unless
Upstash is set. Listen, data-directory, bootstrap and native rows do not
apply. See [Edge Wasm](/deployment/edge/).

## Instance Settings

Runtime settings live in the `settings` table and are edited at
console → Settings (`GET`/`PATCH /admin/api/instance-settings` and
`/admin/api/log-settings`). They take effect without a restart.

| Key | Console label | Default | Meaning |
| --- | --- | --- | --- |
| `instance_name` | Instance name | `default` | Shown in logs and telemetry. |
| `proxy` | Default upstream proxy | none | Used after credential and provider proxies; `GPROXY_UPSTREAM_PROXY_URL` overrides it. |
| `inherit_system_proxy` | Inherit system proxy | off | Honour `HTTP_PROXY`/`HTTPS_PROXY` when no explicit proxy applies. Native only. |
| `enable_usage` | Usage recording | on | Persist usage rows after settlement. Admission and quota accounting run either way. |
| `enable_tokenizer_vocabs` | Use vocabularies | on | Count tokens with a real vocabulary; off falls back to the character estimate. Native only. |
| `enable_tokenizer_download` | Automatic vocabulary fetching | off | Fetch uncached vocabularies from Hugging Face while counting. |
| `default_tokenizer_vocab` | Default vocabulary | none | Used when a model matches no pattern in the provider's `tokenizer_map`. |
| `file_upload_max_in_flight` | Concurrent file uploads | `0` | `0` is unlimited; the environment override wins. |
| `retention_days` | Retention days | unset | Age limit for usage rows, request logs and wire logs. Unset behaves as 36,500 days. |
| `max_database_size_mb` | Database size cap (MiB) | unset | Above the cap the oldest request and wire logs are trimmed; usage rows are never trimmed by size. Unset behaves as 1,024 MiB. |
| `enable_downstream_log`, `enable_downstream_log_body` | Downstream metadata / bodies | — | Record the caller's request and response metadata, and optionally bodies. |
| `enable_upstream_log`, `enable_upstream_log_body` | Upstream metadata / bodies | — | Record every upstream attempt, and optionally bodies. |
| `disable_log_redaction` | Disable log redaction | off | Store captured headers and bodies in clear text. Redaction is on by default. |
| `traffic_blacklist` | Global metadata blacklist | built-in list | Extra request header, response header and query names removed instance-wide, on top of the built-in list. |
| `update_channel`, `enable_auto_update_check` | Update | build channel | Console preference for the update channel and the automatic check. |

The Hugging Face token is stored sealed in its own table
(`tokenizer_auth`), not in `settings`. Login startup and the update
actions on the same console page are served by the native host, not the
database.

## Shutdown

The native binary handles `Ctrl-C` (SIGINT) and SIGTERM. On either signal it
stops accepting connections and lets in-flight requests and streams finish
before it exits; there is no drain deadline. The container image declares
`STOPSIGNAL SIGTERM`, so the normal `docker stop` path is graceful.
