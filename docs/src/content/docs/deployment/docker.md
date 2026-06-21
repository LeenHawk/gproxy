---
title: Docker
description: Run the published GPROXY v2 Docker image with persistent data and optional database DSNs.
---

The official image is `ghcr.io/leenhawk/gproxy`. It is a thin runtime image
built from a prebuilt Linux binary; the Dockerfile does not compile Rust or the
console. The binary already contains the embedded console assets.

## Tags

The release workflow publishes per-architecture images and then creates
multi-architecture manifest lists.

| Tag | Meaning |
| --- | --- |
| `latest` | Latest published release, glibc runtime, amd64 and arm64 manifest. |
| `<release-tag>` | Specific release tag. |
| `latest-musl` | Latest published release, static musl runtime, amd64 and arm64 manifest. |
| `<release-tag>-musl` | Specific release tag, static musl runtime. |
| `latest-amd64`, `latest-arm64` | Architecture-specific glibc images used to build the manifest. |
| `latest-amd64-musl`, `latest-arm64-musl` | Architecture-specific musl images. |

Most deployments should use `latest` or a pinned release tag.

## Quick Run

```bash
docker run --rm \
  --name gproxy \
  -p 8787:8787 \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

Then open <http://127.0.0.1:8787/console> and log in as `admin` with the
password you set.

The image sets:

| Env | Image default |
| --- | --- |
| `GPROXY_HOST` | `0.0.0.0` |
| `GPROXY_PORT` | `8787` |
| `GPROXY_PERSISTENCE` | `file` |
| `GPROXY_DATA_DIR` | `/app/data` |

`file` persistence is local-disk JSON storage, suitable for a single container.
Mount `/app/data` if you want data to survive container replacement.

## Persistent Volume

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/app/data \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

Remove `GPROXY_ADMIN_PASSWORD` after you have created a durable admin password
inside Console. While it is set, the server force-resets the named admin user on
every boot.

## First-Boot Import

To seed providers, routes, credentials, users, and keys from a v2 JSON bundle:

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/app/data \
  -v "$PWD/import.json:/etc/gproxy/import.json:ro" \
  -e GPROXY_IMPORT_FILE=/etc/gproxy/import.json \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

The import file is consumed only when the store is empty. It is skipped on later
boots once users or providers exist.

## SQLite, PostgreSQL, Or MySQL

The native binary defaults to `db` persistence, but the Docker image defaults to
`file`. Set `GPROXY_PERSISTENCE=db` when you want a database backend.

SQLite inside the mounted data directory:

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/app/data \
  -e GPROXY_PERSISTENCE=db \
  -e GPROXY_DATA_DIR=/app/data \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

With `db` persistence and no `GPROXY_DSN`, v2 derives
`sqlite://<data-dir>/gproxy.db?mode=rwc`.

PostgreSQL example:

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -e GPROXY_PERSISTENCE=db \
  -e GPROXY_DSN='postgres://gproxy:secret@postgres.internal:5432/gproxy' \
  -e GPROXY_MASTER_KEY="$GPROXY_MASTER_KEY" \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

Use a standard base64 32-byte `GPROXY_MASTER_KEY` for sealed secrets.

## docker compose

```yaml
services:
  gproxy:
    image: ghcr.io/leenhawk/gproxy:latest
    restart: unless-stopped
    ports:
      - "8787:8787"
    environment:
      GPROXY_ADMIN_PASSWORD: change-me-please
      GPROXY_MASTER_KEY: ${GPROXY_MASTER_KEY:-}
    volumes:
      - gproxy-data:/app/data

volumes:
  gproxy-data:
```

## Upgrade

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
docker stop gproxy
docker rm gproxy
# recreate the container with the same volume and environment
```

Data stays in the mounted volume or external database. If you are migrating from
v1, read [Migrating From v1 To v2](/deployment/v1-to-v2/) before starting the v2
container against the old SQLite file.
