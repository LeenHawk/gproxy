---
title: Release 构建
description: 构建带内嵌 Console 的生产级 GPROXY v2 native 二进制。
---

native release 构建由一个单 crate Rust build 和一个可选 Console 资产构建组成。
release workflow 会执行两者：先构建 `console/`，上传同步后的 `assets/console/`，再为各
native target 构建 `--bin gproxy`。

## 构建 Console

Console 源码、翻译、路由或样式变更后执行：

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..
```

`pnpm build` 会执行：

1. `tsc -b`
2. `vite build`
3. `node ./scripts/sync-to-embed.mjs`

最后一步把 `console/dist/` 复制到 `assets/console/`。native server 通过
`rust-embed` 嵌入该目录，并在 `/console` 提供服务。

## 构建 native 二进制

从仓库根目录执行：

```bash
cargo build --release --bin gproxy
```

输出位置：

```text
target/release/gproxy
```

指定 target：

```bash
cargo build --release --bin gproxy --target x86_64-unknown-linux-gnu
```

release workflow 会构建 Linux glibc、Linux musl、macOS、Windows 和 Android
目标，并对部分二进制执行 `--help` smoke check。

## 运行时配置

二进制通过 CLI flag 和环境变量配置。v2 没有 TOML runtime config 文件。

常用设置：

| CLI | Env | 默认 |
| --- | --- | --- |
| `--host` | `GPROXY_HOST` | `127.0.0.1` |
| `--port` | `GPROXY_PORT` | `8787` |
| `--persistence` | `GPROXY_PERSISTENCE` | `db` |
| `--data-dir` | `GPROXY_DATA_DIR` | `./data` |
| `--dsn` | `GPROXY_DSN` | `<data-dir>/gproxy.db` SQLite |
| `--redis-url` | `GPROXY_REDIS_URL` | 进程内 memory cache |
| `--admin-user` | `GPROXY_ADMIN_USER` | `admin` |
| `--admin-password` | `GPROXY_ADMIN_PASSWORD` | 需要时生成随机 first-boot 密码 |

`GPROXY_MASTER_KEY` 只能通过环境变量设置。需要 v2 对 credentials 和 user API keys
做 at-rest sealing 时，它必须是标准 base64 编码的 32 字节。

## 打包二进制

简单 archive：

```bash
mkdir -p dist
cp target/release/gproxy dist/gproxy
cp README.md dist/
(cd dist && zip -9 ../gproxy-local.zip gproxy README.md)
shasum -a 256 gproxy-local.zip > gproxy-local.zip.sha256
```

release workflow 可能会对部分 Linux 和 Windows artifact 做 UPX 压缩。macOS artifact
使用 `codesign --sign -` 做 ad hoc 签名。

## 首次启动

native server 启动时会：

1. 创建 `GPROXY_DATA_DIR`；
2. 根据 `GPROXY_MASTER_KEY` 创建 secret cipher；
3. 在满足默认 v1-to-v2 条件时自动迁移 v1 SQLite 数据库；
4. 打开配置的 persistence backend；
5. 仅在 providers 和 users 为空时导入 `GPROXY_IMPORT_FILE`；
6. 确保或恢复 admin 用户；
7. 启动 cache、upstream transport、snapshot、router、Console 和 gateway。

用 `./gproxy --help` 查看当前构建的完整 flag。
