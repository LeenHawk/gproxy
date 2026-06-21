# gproxy on Appwrite Functions (deno-2.0 runtime, via wasm)

gproxy runs on Appwrite Functions as a **Deno** function that serves the
**pre-built edge wasm** — Appwrite never compiles Rust. `main.ts` bridges
Appwrite's `context.req` → the wasm `fetch` export → `context.res`. This is the
same wasm that runs on Netlify / Supabase / Deno Deploy.

> **Why not the native Rust runtime?** Appwrite's `rust-1.83` runtime can't build
> gproxy: it compiles your crate with **Cargo 1.83** (gproxy is edition 2024 /
> needs ≥1.85), expects the crate named `handler`, builds with default features,
> and caps build time at ~10 min — all fatal for a crate this size with BoringSSL.
> The wasm/deno path sidesteps every one of those (deploys in ~20 s). **Verified
> live** (admin-gate 401 = the wasm router running).

## Deploy

```bash
# 1. Build the wasm + glue (needs cargo; the glue is gitignored build output)
cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
bash deploy/appwrite-deno/build.sh

# 2. Configure the CLI (an API key avoids interactive/region login)
appwrite client --endpoint https://<region>.cloud.appwrite.io/v1 \
  --project-id <PROJECT_ID> --key <API_KEY>

# 3. Create the function (once) + push the bundle (no Rust build server-side)
appwrite functions create --function-id gproxy-wasm --name gproxy-wasm \
  --runtime deno-2.0 --execute any
appwrite push functions --function-id gproxy-wasm --activate

# 4. Set storage env vars (read by main.ts at cold start)
appwrite functions create-variable --function-id gproxy-wasm \
  --variable-id TURSO_URL --key TURSO_URL --value "<turso-url>"
#   ... TURSO_TOKEN (required), UPSTASH_URL / UPSTASH_TOKEN / GPROXY_MASTER_KEY (optional)
```

The function dir must contain `main.ts` + the generated `gproxy.js` +
`gproxy_wasm_inline.ts` (self-contained). Invoke via the executions API or a
function domain; the gproxy router serves the request by path.
