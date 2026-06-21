# Deployment Targets

All platform-specific deployment entries live under this directory. Keep the
crate root focused on Rust source and shared build outputs.

- `cloudflare/` - Cloudflare Workers entry and wrangler config.
- `deno/` - Deno Deploy entry.
- `eopages/` - Tencent EdgeOne Pages spike and entry. The old Tencent EdgeOne
  TEO/CDN probe was removed; Pages is the only EdgeOne target kept here.
- `netlify/` - Netlify Edge Function entry, config, and minimal publish dir.
- `supabase/` - Supabase Edge Function entry and config.
- `appwrite-deno/` - Appwrite Functions (deno-2.0) entry, serving the prebuilt wasm.

Run build scripts from the crate root (`/home/linhuan/gproxy/v2`). Run provider
CLIs from their own `deploy/<provider>/` directories unless that provider's
notes say otherwise.
