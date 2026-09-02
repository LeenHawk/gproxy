---
title: "Container"
description: "Run the ghcr.io/leenhawk/gproxy image with a persistent volume, external databases, a shared cache, a reverse proxy, and JSON logs"
---

The official image is `ghcr.io/leenhawk/gproxy:<tag>`, where `<tag>` is a
release tag such as `v3.0.0-alpha.0`. The release workflow builds one
`linux/amd64` image per tag and pushes nothing else: there is no `latest`
tag and no `-musl` variant. Pin the tag you tested. Versions are listed on
[Downloads](/getting-started/downloads/).

The multi-stage `deploy/container/Dockerfile` builds the console in a Node
stage, embeds it and compiles `gproxy` in a Rust stage, and ships a
`debian:trixie-slim` runtime with only `ca-certificates`. The binary is the
native release binary compiled with installation kind `container`.

## Image Defaults

| Setting | Value |
| --- | --- |
| User | `gproxy` (system user, home `/var/lib/gproxy`) |
| Port | `EXPOSE 8787` |
| Entrypoint | `/usr/local/bin/gproxy`; extra arguments go to the binary |
| `GPROXY_HOST` | `0.0.0.0` |
| `GPROXY_PORT` | `8787` |
| `GPROXY_DATA_DIR` | `/var/lib/gproxy` |
| `GPROXY_PERSISTENCE` | `sqlite` |

With the defaults the database is `/var/lib/gproxy/gproxy.db`. Mount
`/var/lib/gproxy` or the data is lost with the container. A named volume
takes the directory's ownership from the image; a bind-mounted host
directory must be writable by the `gproxy` user.

## Quick Run

```sh
docker run -d --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/var/lib/gproxy \
  ghcr.io/leenhawk/gproxy:v3.0.0-alpha.0
```

Open `http://127.0.0.1:8787/admin`. A fresh store shows the setup form that
creates the first administrator; afterwards the same address is the login
page. To skip the form, pass the first-run variables:

```sh
docker run -d --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/var/lib/gproxy \
  -e GPROXY_ADMIN_USER=admin \
  -e GPROXY_ADMIN_PASSWORD='<choose-a-password>' \
  ghcr.io/leenhawk/gproxy:v3.0.0-alpha.0
```

On a fresh store this creates the administrator and a sealed admin API key
that you reveal in the console. `GPROXY_BOOTSTRAP_ADMIN_API_KEY` supplies
that key yourself and `GPROXY_BOOTSTRAP_CHANNELS` creates one empty provider
per channel id; both require `GPROXY_ADMIN_PASSWORD` on a fresh store. While
`GPROXY_ADMIN_PASSWORD` stays set, the named administrator's password is
reapplied on every start, so remove it once you have logged in.

The binary also reads `GPROXY_*` keys from `<data-dir>/.env`, here
`/var/lib/gproxy/.env` inside the volume; a variable set with `-e` wins.

## Compose

```yaml
services:
  gproxy:
    image: ghcr.io/leenhawk/gproxy:v3.0.0-alpha.0
    restart: unless-stopped
    ports:
      - "8787:8787"
    environment:
      GPROXY_LOG_FORMAT: json
      # GPROXY_MASTER_KEY: "<standard base64, 32 bytes>"
    volumes:
      - gproxy-data:/var/lib/gproxy

volumes:
  gproxy-data:
```

## Secrets at Rest

Set `GPROXY_MASTER_KEY` to a standard-base64 32-byte key
(`openssl rand -base64 32`) to seal credentials and user keys with
AES-256-GCM. Unset means plaintext, which is the right choice when the
database itself is trusted. Startup refuses a sealed store without the key
that sealed it. Rotation uses `GPROXY_MASTER_KEY_NEXT` and
`GPROXY_MASTER_KEY_ROTATE`; see [Configuration](/reference/configuration/).

## External Databases and Cache

| Backend | Variables |
| --- | --- |
| PostgreSQL | `GPROXY_PERSISTENCE=postgres`, `GPROXY_DSN=postgres://gproxy:<password>@db:5432/gproxy` |
| MySQL | `GPROXY_PERSISTENCE=mysql`, `GPROXY_DSN=mysql://gproxy:<password>@db:3306/gproxy` |
| libSQL / Turso | `GPROXY_PERSISTENCE=libsql`, `GPROXY_LIBSQL_URL=https://<db>-<org>.turso.io`, `GPROXY_LIBSQL_AUTH_TOKEN=<token>` |
| Redis cache | `GPROXY_REDIS_URL=redis://cache:6379` (or `rediss://`) |
| Upstash cache | `UPSTASH_URL=https://<name>.upstash.io`, `UPSTASH_TOKEN=<token>`; set both or neither |

The PostgreSQL connection is opened without TLS, so keep the database on a
private network. The libSQL URL must be an absolute `http(s)` URL; the store
speaks Hrana over HTTP, and with `libsql` persistence the cache is a libSQL
table unless Redis or Upstash is configured. The default cache is
in-process: running more than one replica requires Redis or Upstash so that
quotas, rate limits, and OAuth refresh leases are shared. Keep the volume
even with an external database; the data directory is still created and
holds `.env`. See [Storage & Cache Backends](/reference/database/).

```yaml
services:
  gproxy:
    image: ghcr.io/leenhawk/gproxy:v3.0.0-alpha.0
    ports:
      - "8787:8787"
    environment:
      GPROXY_PERSISTENCE: postgres
      GPROXY_DSN: postgres://gproxy:<password>@db:5432/gproxy
      GPROXY_REDIS_URL: redis://cache:6379
    depends_on: [db, cache]
  db:
    image: postgres:17
    environment:
      POSTGRES_USER: gproxy
      POSTGRES_PASSWORD: <password>
      POSTGRES_DB: gproxy
  cache:
    image: redis:7
```

## Behind a Reverse Proxy

The image serves plain HTTP; terminate TLS in front of it. Two variables
tell the gateway about the proxy:

| Variable | Effect |
| --- | --- |
| `GPROXY_TRUSTED_PROXIES` | Comma-separated IP addresses (not CIDR ranges). When the TCP peer is loopback or one of these, the first `X-Forwarded-For` entry, or else `X-Real-IP`, becomes the client IP used for login throttling and audit. From any other peer the headers are ignored. |
| `GPROXY_CORS_ORIGINS` | Exact browser origins allowed to call the API cross-site with credentials. Empty means same-origin only, which is enough when the console and portal are served by the gateway itself. |

On a compose network give the proxy container a fixed address so it can be
listed. Forward the `Upgrade` and `Connection` headers for WebSocket
clients, disable response buffering for streaming, and allow request bodies
up to 100 MiB, the gateway's own limit.

## Health and Version

There is no dedicated health endpoint. Two unauthenticated requests serve
the purpose:

| Request | Meaning |
| --- | --- |
| `GET /build-info.js` | `200` with `globalThis.__GPROXY_BUILD_INFO__ = {version, channel, buildHash, installationKind}`; the process is up |
| `GET /admin/api/session` | `200` with `{"setup_required":false,"user":null}`; the database answers |

`docker run --rm ghcr.io/leenhawk/gproxy:<tag> --version` prints the build
identity. The image has no `curl` or `wget`, so probe from the host or the
orchestrator (a Kubernetes `httpGet` probe) rather than with a `HEALTHCHECK`.

## Logs

The binary logs to stdout. `GPROXY_LOG_FORMAT=json` switches from text to
newline-delimited JSON; `RUST_LOG` sets the filter (default `info`). Read
them with `docker logs -f gproxy`. Request audit and wire capture live in
the database, not in the log; see
[Usage, Logs & Audit](/guides/observability/).

## Graceful Stop

The binary shuts down cleanly on `SIGINT` and `SIGTERM`: it stops accepting
connections and lets in-flight requests finish. The image declares
`STOPSIGNAL SIGTERM`, so an ordinary `docker stop gproxy` uses this graceful
path.

## Upgrade

```sh
docker pull ghcr.io/leenhawk/gproxy:<new-tag>
docker stop gproxy && docker rm gproxy
# recreate the container with the same volume and environment
```

Data stays in the volume or the external database. Coming from a v2
container, read [v2 to v3 Migration](/deployment/v2-to-v3/) first; the v2
image kept its data under `/app/data`, and the migration is a subcommand of
the same entrypoint:

```sh
docker run --rm \
  -v gproxy-data:/var/lib/gproxy \
  -v gproxy-v2-data:/v2:ro \
  ghcr.io/leenhawk/gproxy:v3.0.0-alpha.0 \
  migrate --from-v2 /v2/gproxy.db
```

## Build the Image Locally

The Dockerfile at `deploy/container/Dockerfile` refuses to build without
`GPROXY_UPDATE_PUBKEY`, and the value must decode to exactly 32 bytes. For a
local image, generate a throwaway key:

```sh
PUBKEY="$(openssl genpkey -algorithm ed25519 \
  | openssl pkey -pubout -outform DER | tail -c 32 | base64 -w0)"
docker buildx build \
  -f deploy/container/Dockerfile \
  --build-arg GPROXY_UPDATE_PUBKEY="$PUBKEY" \
  --build-arg GPROXY_BUILD_VERSION=3.0.0-local \
  --build-arg GPROXY_BUILD_CHANNEL=dev \
  --build-arg GPROXY_BUILD_HASH="$(git rev-parse HEAD)" \
  -t gproxy:local .
```

| Build arg | Default | Purpose |
| --- | --- | --- |
| `GPROXY_UPDATE_PUBKEY` | required | Ed25519 public key compiled into the binary |
| `GPROXY_BUILD_VERSION`, `GPROXY_BUILD_HASH` | unset | Build identity shown by `--version` |
| `GPROXY_BUILD_CHANNEL` | `releases` | Default update channel |
| `GPROXY_INSTALLATION_KIND` | `container` | Installation kind shown by `--version` |
| `GPROXY_VERSION`, `GPROXY_REVISION` | unset | OCI image labels |
| `CARGO_NET_OFFLINE` | `false` | Build from a warmed cargo cache |

The build needs no prebuilt console; the first stage compiles it.
`docker buildx build -f deploy/container/Dockerfile --target console-dist --output type=local,dest=dist/console .`
exports only the console bundle; the release workflow uses it to build the
console once for every other job.

## Load the Release Archive

Each release also publishes the pushed image as a file for hosts without
registry access:

```sh
sha256sum -c gproxy-container-linux-amd64.tar.gz.sha256
docker load -i gproxy-container-linux-amd64.tar.gz
```

The loaded image carries its original tag, `ghcr.io/leenhawk/gproxy:<tag>`.
`gproxy-container-linux-amd64.provenance.json` beside it records the commit
and the base image digests; see
[Building & Releases](/deployment/release-build/).
