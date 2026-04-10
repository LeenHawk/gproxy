---
title: Deployment
description: Run GPROXY as a binary, Docker container, or with external databases.
---

## Binary

Download the release binary for your platform from [GitHub Releases](https://github.com/LeenHawk/gproxy/releases).

```bash
chmod +x gproxy
./gproxy
```

That's it. GPROXY starts with sensible defaults:

- Listens on `127.0.0.1:8787`
- Creates a SQLite database at `./data/gproxy.db`
- Auto-generates an admin user, password, and API key (logged at startup -- save them)
- Serves the admin console at `http://127.0.0.1:8787/`

To provide your own admin credentials:

```bash
./gproxy \
  --admin-user admin \
  --admin-password 'your-password' \
  --admin-api-key 'your-api-key'
```

## Docker

```bash
docker pull ghcr.io/leenhawk/gproxy:latest

docker run --rm -p 8787:8787 \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_PORT=8787 \
  -e GPROXY_ADMIN_USER=admin \
  -e GPROXY_ADMIN_PASSWORD='your-password' \
  -e DATABASE_SECRET_KEY='your-encryption-key' \
  -v $(pwd)/data:/app/data \
  ghcr.io/leenhawk/gproxy:latest
```

### Image variants

| Tag | Base | Use case |
|-----|------|----------|
| `latest` | glibc | Standard deployments |
| `latest-musl` | musl (static) | Alpine, scratch, or minimal containers |

Both variants are available for `amd64` and `arm64`.

### Persistent storage

Mount `/app/data` as a volume. This is where the default SQLite database and any file-based data live.

```bash
-v $(pwd)/data:/app/data
```

To use an external database instead of SQLite, set `GPROXY_DSN` and skip the volume mount (unless other file-based features need it):

```bash
docker run --rm -p 8787:8787 \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_ADMIN_USER=admin \
  -e GPROXY_ADMIN_PASSWORD='your-password' \
  -e GPROXY_DSN='mysql://user:password@db-host:3306/gproxy' \
  -e DATABASE_SECRET_KEY='your-encryption-key' \
  ghcr.io/leenhawk/gproxy:latest
```

### Database encryption in Docker

Inject `DATABASE_SECRET_KEY` via environment variable, Docker secrets, or your platform's secrets management. Set it before first startup so all sensitive fields are encrypted from the start. See [Configuration Reference](/guides/configuration/#database-encryption) for details.

## External databases

GPROXY supports SQLite, MySQL, and PostgreSQL. Set `GPROXY_DSN` with the appropriate connection string:

```bash
# MySQL
GPROXY_DSN='mysql://user:password@127.0.0.1:3306/gproxy'

# PostgreSQL
GPROXY_DSN='postgres://user:password@127.0.0.1:5432/gproxy'
```

Schema is auto-synced on startup. No manual migration required.
