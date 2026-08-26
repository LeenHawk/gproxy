# GPROXY v3

The from-scratch 3.0 rewrite. The beta release pipeline builds the native
gateway, embedded console, container image, installers, Android app, and edge
bundles from one tagged commit.

- **v2 (current stable)** lives on the [`main`](https://github.com/LeenHawk/gproxy/tree/main)
  branch and its `v2.x.y` tags — releases and maintenance continue there.
- The v3 design goals: an embeddable `gproxy-core` (channels, credential
  lifecycle, transforms, execution) consumed by interchangeable hosts
  (axum server / edge wasm) and embeddable into other applications.

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
