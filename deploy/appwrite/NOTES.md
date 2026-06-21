# gproxy on Appwrite Functions (Rust 1.83)

`deploy/appwrite/` wraps gproxy's native axum router as an Appwrite **Rust 1.83**
function. The handler converts the Appwrite request → `http::Request` → gproxy's
`router.oneshot()` → `http::Response`, building `AppState` once per instance.

**Status:** the adapter **compiles** against the real gproxy router + the
`openruntimes-types-for-rust` API. A live Appwrite deployment is **not yet
verified** — the two open items below.

## Configuration (function environment variables)

| Var | Required | Purpose |
|---|---|---|
| `GPROXY_DSN` | ✅ | Postgres/MySQL DSN for the control plane (serverless has no durable local disk) |
| `GPROXY_MASTER_KEY` | — | unseal stored secrets (absent = plaintext) |
| `GPROXY_UPSTREAM_PROXY_URL` | — | outbound proxy for upstream calls |
| `GPROXY_ADMIN_USER` / `GPROXY_ADMIN_PASSWORD` | — | first-boot admin |

## Deploy (CLI)

```bash
# 1. Point the CLI at your project (an API key avoids interactive/region login):
appwrite client --endpoint https://<region>.cloud.appwrite.io/v1 \
  --project-id <PROJECT_ID> --key <API_KEY>

# 2. Push (see "Packaging" — the function needs the gproxy crate in its build
#    context, so a GitHub-connected function with root directory deploy/appwrite
#    is the clean path).
appwrite push function
```

## Open items

1. **Packaging.** The adapter `path`-depends on the gproxy crate (`../..`). A
   plain `appwrite push` tars only the function directory, so the parent crate is
   missing at build time. Use Appwrite's **GitHub integration** (clones the whole
   repo, builds the sub-crate at root directory `deploy/appwrite`), or fold the
   handler into the root crate behind a feature.
2. **Native build deps.** gproxy's native build pulls **BoringSSL** (via `wreq`,
   needs cmake + Go) and **onig** (via `tokenizers`, needs a C toolchain). Whether
   Appwrite's `rust-1.83` build container provides these is unconfirmed; if not,
   the function won't compile there.
