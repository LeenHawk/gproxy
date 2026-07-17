# Deployment Targets

All platform-specific deployment entries live under this directory. Keep the
crate root focused on Rust source and shared build outputs.

- `cloudflare/` - Cloudflare Workers entry and wrangler config.
- `deno/` - Deno Deploy entry.
- `netlify/` - Netlify Edge Function entry, config, and Console publish dir.

Run build scripts from the crate root (`/home/linhuan/gproxy/v2`). Run provider
CLIs from their own `deploy/<provider>/` directories unless that provider's
notes say otherwise.
