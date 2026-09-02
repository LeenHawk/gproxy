# GPROXY v3

The from-scratch 3.0 rewrite. The beta release pipeline builds the native
gateway, embedded console, container image, installers, Android app, and edge
bundles from one tagged commit.

- **v2 (current stable)** lives on the [`main`](https://github.com/LeenHawk/gproxy/tree/main)
  branch and its `v2.x.y` tags — releases and maintenance continue there.
- The v3 design goals: an embeddable `gproxy-core` (channels, credential
  lifecycle, transforms, execution) consumed by interchangeable hosts
  (axum server / edge wasm) and embeddable into other applications.

## Run locally

```sh
cargo run -p gproxy-host-axum
```

No configuration file or encryption key is required. The native host reads
`GPROXY_*` variables from the real process environment, then optionally from
`.env` in the current directory and the data directory. A real environment
variable always wins. The defaults are `127.0.0.1:8787`, `./data`, SQLite, and
plaintext secret storage. `GPROXY_LIBSQL_URL` and
`GPROXY_LIBSQL_AUTH_TOKEN` are required only when
`GPROXY_PERSISTENCE=libsql`.

Set `GPROXY_MASTER_KEY` to a standard-base64 32-byte key to encrypt stored
credential and user-key material. To rotate, keep the current key in
`GPROXY_MASTER_KEY`, set the target in `GPROXY_MASTER_KEY_NEXT`, and explicitly
arm one restart with `GPROXY_MASTER_KEY_ROTATE=on`. An empty `NEXT` rotates to
plaintext. Rotation re-seals every secret and updates the key fingerprint in
one database transaction; after success, move the target into
`GPROXY_MASTER_KEY` (or unset it for plaintext) and clear `NEXT` and `ROTATE`.
The stored fingerprint is never a key, and startup refuses a sealed store when
the required fingerprint is not supplied.

## Documentation

The documentation site lives under `docs/` (Astro + Starlight) and is
published to <https://gproxy.leenhawk.com> by CI. It also hosts the signed
announcement feed the native binary polls. Run it locally with:

```sh
cd docs
pnpm install --frozen-lockfile
pnpm dev
```

## Release

Configure the Android and update-signing CI secrets named in
`.github/workflows/release.yml`, then run from a clean, versioned commit:

```sh
scripts/release.sh
```

The script derives `vX.Y.Z` from `[workspace.package].version`, creates the
annotated tag if needed, and pushes only that tag. The tag workflow builds the
console once and publishes the GHCR image plus checksummed native, installer,
signed APK, signed manifest, and edge assets. Re-running the command is safe
when the same tag already points at the same commit.

3.0 的 beta 发布由同一个 tag 构建原生程序、控制台、容器、安装包、Android
应用和 edge bundles。v2 稳定版仍在 `main` 分支与 `v2.x.y` tags 上维护。
