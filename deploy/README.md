# Deployment Targets

Platform-specific source entries live under this directory. Release CI turns
them into self-contained, ready-to-deploy platform directories on the orphan
`deploy` branch; that branch contains prebuilt wasm/glue and does not compile
Rust on Cloudflare, Deno Deploy, or Netlify.

- `cloudflare/` - Cloudflare Workers entry and wrangler config.
- `deno/` - Deno Deploy entry.
- `netlify/` - Netlify Edge Function entry, config, and Console publish dir.

Each deployed platform directory is runtime-independent: it must not import from
a sibling platform directory or from the main branch's `deploy/` root.
`edge-runtime.js` is the source of truth for the shared environment contract and
init-once adapter. Existing build/release scripts run `stage-edge-runtime.sh` to
copy it to a platform-local `_shared.js` before packaging.

| Variable | Required | Purpose |
| --- | --- | --- |
| `TURSO_URL` | Yes | libSQL/Turso HTTP URL. |
| `TURSO_TOKEN` | Yes | Turso access token. |
| `GPROXY_ADMIN_USER` | Yes | First-boot Console admin username. |
| `GPROXY_ADMIN_PASSWORD` | Yes | First-boot Console admin password. |
| `UPSTASH_URL` | No | Upstash Redis cache URL. |
| `UPSTASH_TOKEN` | No | Upstash access token. |
| `GPROXY_MASTER_KEY` | No | Standard base64 32-byte sealed-secret key. |

Run build scripts from the repository root. Run provider CLIs from their own
`deploy/<provider>/` directories unless that provider's notes say otherwise.
See `docs/src/content/docs/deployment/edge.md` and its `zh-cn` counterpart for
bundle shapes and deployment steps.
