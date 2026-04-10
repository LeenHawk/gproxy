---
title: Deployment
description: Deploy GPROXY locally (binary, Docker) and in cloud (ClawCloud Run).
---

## Local deployment

### Binary

1. Download the release binary from [GitHub Releases](https://github.com/LeenHawk/gproxy/releases).
2. Prepare config file:

```bash
cp gproxy.example.toml gproxy.toml
```

3. Start service:

```bash
./gproxy
```

After startup, open:

- Admin UI: `http://127.0.0.1:8787/`

### Docker

Pull prebuilt image (recommended):

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
```

Run container:

```bash
docker run --rm -p 8787:8787 \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_PORT=8787 \
  -e GPROXY_ADMIN_KEY=your-admin-key \
  -e DATABASE_SECRET_KEY='replace-with-long-random-string' \
  -e GPROXY_DSN='sqlite:///app/data/gproxy.db?mode=rwc' \
  -v $(pwd)/data:/app/data \
  ghcr.io/leenhawk/gproxy:latest
```

> Inject `DATABASE_SECRET_KEY` via Docker secrets, platform secrets, or env vars. Especially on free-tier or shared managed databases, configure it before first bootstrap so sensitive fields are not stored in plaintext.

## Cloud deployment

### ClawCloud Run

Current cloud template support is ClawCloud Run.

- Template file: [`claw.yaml`](https://github.com/LeenHawk/gproxy/blob/main/claw.yaml)
- Prebuilt image: `ghcr.io/leenhawk/gproxy:latest`
- Use the template in ClawCloud Run App Store -> My Apps -> Debugging

Recommended inputs:

- `admin_key` (default: generated random value)
- `rust_log` (`info`)
- `volume_size` (`1`)
- Configure `DATABASE_SECRET_KEY` through platform secrets
- Persist volume at `/app/data`

Built-in environment defaults:

- `GPROXY_HOST=0.0.0.0`
- `GPROXY_PORT=8787`
- `GPROXY_DSN=sqlite:///app/data/gproxy.db?mode=rwc`

Optional inputs:

- `proxy_url` (upstream egress proxy)

### Release downloads and self-update (Cloudflare Pages)

- The release workflow also deploys a dedicated Cloudflare Pages downloads site for binaries and update manifests.
- Default public base URL: `https://download-gproxy.leenhawk.com`
- Generated manifests:
  - `/manifest.json` — full file index for the docs downloads page
  - `/releases/manifest.json` — stable self-update channel
  - `/staging/manifest.json` — staging self-update channel
- The admin UI `Cloudflare` update source reads from this site.
- Required repository secrets for the downloads deployment:
  - `CLOUDFLARE_API_TOKEN`
  - `CLOUDFLARE_ACCOUNT_ID`
  - `CLOUDFLARE_DOWNLOADS_PROJECT_NAME`
- Optional repository secrets:
  - `DOWNLOAD_PUBLIC_BASE_URL`
  - `UPDATE_SIGNING_KEY_ID`
  - `UPDATE_SIGNING_PRIVATE_KEY_B64`
  - `UPDATE_SIGNING_PUBLIC_KEY_B64`
