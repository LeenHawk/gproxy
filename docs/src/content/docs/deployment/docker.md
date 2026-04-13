---
title: Docker
description: Running GPROXY in a container with the official pre-built image from GHCR.
---

The official GPROXY container image is published to GitHub Container Registry as **`ghcr.io/leenhawk/gproxy`** by the release pipeline. Pull it — do **not** rebuild it locally unless you're working on GPROXY itself.

## Image tags

| Tag | When it moves | Notes |
|---|---|---|
| `latest` | Published releases | Stable, multi-arch (amd64 + arm64), glibc base. Most users want this. |
| `v1.2.3` | Tagged at release time | Pin to a specific version for reproducible deployments. |
| `staging` | Every push to `main` | Latest pre-release from `main`, multi-arch glibc. Use only if you want bleeding-edge fixes. |
| `latest-musl` / `v1.2.3-musl` / `staging-musl` | Mirrors of the above | Static-musl flavor; slightly smaller runtime base, no glibc dependency. |

Per-architecture tags (`latest-amd64`, `latest-arm64`, `-musl` suffixes) also exist, but the un-suffixed manifest list is a multi-arch image and Docker will pick the right one automatically — prefer the short form.

## Pull

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
```

No authentication is required; the image is public.

## Run

GPROXY needs a place to persist its data directory (the SQLite file, if you're using SQLite). Mount a volume and pass the usual environment variables:

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/var/lib/gproxy \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_PORT=8787 \
  -e GPROXY_DATA_DIR=/var/lib/gproxy \
  -e GPROXY_CONFIG=/etc/gproxy/seed.toml \
  -e GPROXY_ADMIN_USER=admin \
  -e GPROXY_ADMIN_PASSWORD=change-me \
  -v "$PWD/seed.toml:/etc/gproxy/seed.toml:ro" \
  ghcr.io/leenhawk/gproxy:latest
```

A few notes on the above:

- **Bind to `0.0.0.0` inside the container**, otherwise the listener won't be reachable from outside the container.
- **`GPROXY_DATA_DIR`** should point somewhere inside a persistent volume. The default `./data` lives in the container's working directory and is lost on container replacement.
- **`GPROXY_CONFIG`** is only needed on the first run; after that, the database in the volume is authoritative and the seed file is ignored.

## With PostgreSQL

Point `GPROXY_DSN` at your database and skip the SQLite volume:

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_DSN=postgres://gproxy:secret@postgres.internal:5432/gproxy \
  -e DATABASE_SECRET_KEY=$(cat gproxy-db-key) \
  -e GPROXY_ADMIN_USER=admin \
  -e GPROXY_ADMIN_PASSWORD=change-me \
  ghcr.io/leenhawk/gproxy:latest
```

## docker-compose example

```yaml
services:
  gproxy:
    image: ghcr.io/leenhawk/gproxy:latest
    restart: unless-stopped
    ports:
      - "8787:8787"
    environment:
      GPROXY_HOST: 0.0.0.0
      GPROXY_PORT: "8787"
      GPROXY_DATA_DIR: /var/lib/gproxy
      GPROXY_CONFIG: /etc/gproxy/seed.toml
      GPROXY_ADMIN_USER: admin
      GPROXY_ADMIN_PASSWORD: change-me
    volumes:
      - gproxy-data:/var/lib/gproxy
      - ./seed.toml:/etc/gproxy/seed.toml:ro

volumes:
  gproxy-data:
```

## Upgrading

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
docker stop gproxy && docker rm gproxy
# re-run the `docker run` command from above
```

The data volume is preserved across container replacement, so your database, credentials, and logs survive the upgrade. If you pinned to `v1.2.3`, bump the tag in the pull/run commands to the new version.

## Shutdown behavior

Docker sends `SIGTERM` to the main process on `docker stop`. GPROXY handles it exactly like a Ctrl+C — Axum drains in-flight requests, `UsageSink` writes its final batch, and the process exits. Give it enough grace time (Docker default is 10 s, which is fine); see [Graceful Shutdown](/reference/graceful-shutdown/) for the full sequence.

## Building from source (contributors only)

If you're developing GPROXY and want to test Dockerfile changes, the repository ships [`Dockerfile.action`](https://github.com/LeenHawk/gproxy/blob/main/Dockerfile.action) — the same file the release pipeline uses. Build it locally with:

```bash
docker build -f Dockerfile.action -t gproxy:dev .
```

End users shouldn't need this path — pull `ghcr.io/leenhawk/gproxy:latest` instead.
