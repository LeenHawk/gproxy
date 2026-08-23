# GPROXY v3

The from-scratch 3.0 rewrite. Work in progress; nothing here is usable yet.

- **v2 (current stable)** lives on the [`main`](https://github.com/LeenHawk/gproxy/tree/main)
  branch and its `v2.x.y` tags — releases and maintenance continue there.
- The v3 design goals: an embeddable `gproxy-core` (channels, credential
  lifecycle, transforms, execution) consumed by interchangeable hosts
  (axum server / edge wasm) and embeddable into other applications.

3.0 的从零重写分支,尚不可用。v2 稳定版在 `main` 分支与 `v2.x.y` tags 上继续维护发布。
