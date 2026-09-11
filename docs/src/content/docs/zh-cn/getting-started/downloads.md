---
title: 下载
description: "GPROXY 的发布产物：原生安装包、便携压缩包、容器镜像、Edge Bundle、校验值、构建来源记录和签名更新清单。"
---

每个版本都以 `v3.x.y` tag 发布在 GitHub Releases 页面：

<https://github.com/LeenHawk/gproxy/releases>

[Code signing policy（代码签名政策）](/zh-cn/deployment/code-signing/)：
SignPath Foundation 申请正在等待审核，已有下载不会自动补签。

:::note[稳定版与预发布版链接]
`releases/latest` 以及所有 `releases/latest/download/...` URL 都解析到最新的 v3
稳定版。该链接不包含预发布版本；如需预发布版本，请打开 Release 列表自行选择。
v2 版本仍保留在各自的 `v2.x.y` tag 上。
:::

不要为了运行 GPROXY 而克隆仓库或自行编译。下面的产物已经包含优化后的二进制和内嵌
控制台。从源码构建见[构建与发布](/zh-cn/deployment/release-build/)。

## 产物命名

原生产物命名为 `gproxy-<os>-<arch>[-musl].<ext>`。一个版本包含以下目标：

| 产物名前缀 | 目标 | 便携版 | 安装包 |
| --- | --- | --- | --- |
| `gproxy-linux-x86_64` | x86_64-unknown-linux-gnu | `.zip` | `.deb` |
| `gproxy-linux-aarch64` | aarch64-unknown-linux-gnu | `.zip` | `.deb` |
| `gproxy-linux-riscv64` | riscv64gc-unknown-linux-gnu | `.zip` | `.deb` |
| `gproxy-linux-x86_64-musl` | x86_64-unknown-linux-musl | `.zip` | `.deb` |
| `gproxy-linux-aarch64-musl` | aarch64-unknown-linux-musl | `.zip` | `.deb` |
| `gproxy-linux-riscv64-musl` | riscv64gc-unknown-linux-musl | `.zip` | `.deb` |
| `gproxy-macos-x86_64` | x86_64-apple-darwin | `.zip` | `.dmg` |
| `gproxy-macos-aarch64` | aarch64-apple-darwin | `.zip` | `.dmg` |
| `gproxy-windows-x86_64` | x86_64-pc-windows-msvc | `.zip` | MSIX（商店提交） |
| `gproxy-windows-aarch64` | aarch64-pc-windows-msvc | `.zip` | MSIX（商店提交） |
| `gproxy-android-x86_64` | x86_64-linux-android | `.zip` | `.apk` |
| `gproxy-android-aarch64` | aarch64-linux-android | `.zip` | `.apk` |

Linux GNU 版本链接 glibc；`-musl` 版本是静态链接。Windows 版本静态链接 C 运行时。

## 原生安装包

| 安装包 | 平台 | 作用 |
| --- | --- | --- |
| `.deb` | Debian 与 Ubuntu 系 | 安装 `/usr/bin/gproxy`、桌面启动器和一个 XDG 自动启动项。 |
| `.dmg` | macOS 11 或更高 | `GPROXY.app` 应用包，在后台运行服务并打开控制台。 |
| `.msix` | Windows 10 2004 或更高版本 | 由商店管理安装，带开始菜单入口、私有数据目录和 Windows 启动任务；尚未上架商店。 |
| `.apk` | Android 9（API 28）或更高 | 已签名的应用，包含前台服务、启动器界面和应用内更新。 |

各安装包的行为、数据位置和日志路径见[安装](/zh-cn/getting-started/installation/)。

Windows MSI 打包已改为商店提交用 MSIX。未签名 MSIX 仅保存在 Actions 产物中，
不作为公开 Release 下载；商店审核完成前请使用 Windows 便携 ZIP。

## 便携压缩包

每个 `.zip` 包含可执行文件（`gproxy`，Windows 上是 `gproxy.exe`）、`README.md` 和
`LICENSE`。Android 压缩包包含 `gproxy.bin`、一个 `gproxy` 启动脚本和
`libc++_shared.so`；三个文件要放在一起。

解压后运行：

```bash
chmod +x ./gproxy
./gproxy --help
```

## 容器镜像

```bash
docker pull ghcr.io/leenhawk/gproxy:<tag>
```

`<tag>` 是该版本的 Git tag。工作流向 GHCR 发布 `linux/amd64`、`linux/arm64` 和
`linux/riscv64` 的 GNU 与 `-musl` 变体。容器镜像不作为 Release 附件上传。
离线传输、卷和环境变量见[容器部署](/zh-cn/deployment/docker/)。

## Edge Bundle

| 产物 | 内容 |
| --- | --- |
| `gproxy-edge-cloudflare.zip` | Cloudflare Workers 项目：Worker 入口、`wrangler.toml`、wasm 包、控制台资源。 |
| `gproxy-edge-deno.zip` | Deno Deploy 项目：`main.ts`、`deno.json`、wasm 包、控制台资源。 |
| `gproxy-edge-netlify.zip` | Netlify Edge 项目：Edge Function、`netlify.toml`、wasm 包、控制台资源。 |
| `gproxy-edge.wasm` | 原始的 `wasm32-unknown-unknown` 构建，供自定义宿主使用。 |

请直接上传 Bundle，不要让平台编译 Rust。见 [Edge Wasm](/zh-cn/deployment/edge/)。

## 校验值与构建来源

新构建使用 GitHub Release 附件自带的 `digest`（SHA-256），不再单独上传
`.sha256` 文件。查询所下载版本的摘要：

```bash
gh api 'repos/LeenHawk/gproxy/releases/tags/v<VERSION>' \
  --jq '.assets[] | select(.name == "gproxy-linux-x86_64.zip") | .digest'
sha256sum gproxy-linux-x86_64.zip
```

将本地哈希与 `sha256:` 后的值比较。构建来源通过 GitHub 签名的产物证明验证：

```bash
gh attestation verify gproxy-linux-x86_64.zip -R LeenHawk/gproxy
```

工具链版本和实际解析到的基础镜像摘要保存在另一份自定义证明中，见
[构建溯源](/zh-cn/deployment/release-build/#构建溯源)。历史版本保留原有的校验和与溯源附件。

## 签名更新清单

`manifest.json` 是内置更新器读取的 Ed25519 签名清单。它列出通道、版本、发布说明
URL、最低兼容数据版本，以及每个目标一条记录（URL、SHA-256、大小）。公钥在构建时
嵌入二进制，因此一个二进制只接受由它自己的发布流水线签名的清单。

| 通道 | 清单位置 | 内容 |
| --- | --- | --- |
| `releases` | `releases/latest/download/manifest.json` | 不带预发布后缀的稳定 tag，`v3.0.0` 及之后。稳定版构建默认使用此通道。 |
| `staging` | `releases/download/staging/manifest.json` | `main` 每次推送都会持续替换的构建，按构建哈希而非版本号比较。 |
| `dev` | `releases/download/dev/manifest.json` | 最新的 `v3` 预发布版本。预发布构建默认使用此通道。 |

由预发布 tag 构建的版本会作为 GitHub 预发布发布，并刷新保存最新签名清单的 `dev`
Release。如何应用更新见[安装](/zh-cn/getting-started/installation/#更新)。

`gproxy --version` 输出版本、通道、构建哈希和安装类型：

```text
gproxy 3.0.0 (channel releases, build 4054fe4f94ea, installation standalone)
```

发布构建的通道为 `releases` 或 `dev`，安装类型为 `standalone`、`android-apk` 或
`container`。源码构建输出 `development` 和 `source`。
