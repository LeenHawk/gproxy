---
title: Docker
description: Running gproxy in a container with persistent data and environment-based configuration.
---

The repository ships [`Dockerfile.action`](https://github.com/LeenHawk/gproxy/blob/main/Dockerfile.action),
which is used by the release pipeline to build the official image. You
can build it locally or use it as a reference for your own image.

## Build

```bash
docker build -f Dockerfile.action -t gproxy:local .
```

This produces an image containing the `gproxy` binary with the embedded
console. No separate frontend build step is required in the container —
the frontend is built as part of the Docker build.

## Run

gproxy needs a place to persist its data directory (the SQLite file, if
you're using SQLite). Mount a volume and pass the usual environment
variables:

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
  gproxy:local
```

A few notes on the above:

- **Bind to `0.0.0.0` inside the container**, otherwise the listener
  won't be reachable from outside the container.
- **`GPROXY_DATA_DIR`** should point somewhere inside a persistent
  volume. The default `./data` lives in the container's working
  directory and is lost on container replacement.
- **`GPROXY_CONFIG`** is only needed on the first run; after that, the
  database in the volume is authoritative and the seed file is
  ignored.

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
  gproxy:local
```

## docker-compose example

```yaml
services:
  gproxy:
    image: gproxy:local
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

## Shutdown behavior

Docker sends `SIGTERM` to the main process on `docker stop`. gproxy
handles it exactly like a Ctrl+C — Axum drains in-flight requests,
`UsageSink` writes its final batch, and the process exits. Give it
enough grace time (Docker default is 10 s, which is fine); see
[Graceful Shutdown](/reference/graceful-shutdown/) for the full
sequence.
