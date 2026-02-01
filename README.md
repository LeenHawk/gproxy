# gproxy

gproxy is a Rust-based proxy for multiple AI providers with an embedded admin UI.

## Features
- Multi-provider routing with per-credential management
- Admin API for providers, credentials, users, and keys
- Usage tracking and upstream usage views where available
- Embedded SPA admin UI (React + Tailwind)

## Quick start
```bash
cargo run --release -- --admin-key your-admin-key
```
Open the UI at:
```
http://127.0.0.1:8787/
```

## Configuration
gproxy stores config in the database. By default it uses SQLite under `./data`.

CLI flags:
- `--host <ip>` (default `127.0.0.1`)
- `--port <port>` (default `8787`)
- `--admin-key <key>` (default `pwd`)
- `--dsn <dsn>` (optional, e.g. `sqlite:///path/to/gproxy.db`)
- `--data-dir <dir>` (default `./data`)
- `--proxy <url>` (optional upstream proxy)

Environment:
- `GPROXY_DATA_DIR` (alternative way to set the data dir)

Admin API auth:
- `x-admin-key: <admin_key>` or `Authorization: Bearer <admin_key>`

See `route.md` for API routes and examples.

## Frontend
The admin UI is built from `apps/gproxy/frontend` and embedded from
`apps/gproxy/frontend/dist` at build time.

## Docker
Build:
```bash
docker build -t gproxy:local .
```
Run:
```bash
docker run --rm -p 8787:8787 \
  -e GPROXY_ADMIN_KEY=your-admin-key \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_PORT=8787 \
  -e GPROXY_DATA_DIR=/app/data \
  -v $(pwd)/data:/app/data \
  gproxy:local
```

## License
AGPL-3.0-or-later
