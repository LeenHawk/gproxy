---
title: 安装
description: 通过 release 二进制、Docker 镜像、源码构建或 edge bundle 安装 GPROXY v2。
---

GPROXY v2 是一个单 Rust crate，native 产物是名为 `gproxy` 的二进制。同一个 crate
也可以编译成 edge WebAssembly runtime。native 形态下，React Console 不是独立服务：
构建 `console/` 后，静态文件会同步到 `assets/console/`，再被编进二进制。

按部署形态选择安装方式。

:::tip[优先下载预构建版本]
绝大多数用户应直接前往[下载页](/zh-cn/getting-started/downloads/)或
[最新 GitHub Release](https://github.com/LeenHawk/gproxy/releases/latest)选择安装包。
如果不是要开发 GPROXY 或制作定制版本，请不要从源码构建。
:::

## 原生安装包

Release 除了便携 ZIP，还会发布各平台的原生安装包：

| 平台 | 安装包 | 后台行为 |
| --- | --- | --- |
| Android | `.apk` | Foreground Service 通过常驻通知保持运行。Launcher 控制打开 App 和设备开机后的自动启动，开启时会请求后台运行权限。 |
| Windows | `.msi` | 按用户安装到 Local AppData 并创建开始菜单入口；首次设置可选择是否在登录时隐藏启动。 |
| macOS | `.dmg` | 把 `GPROXY.app` 拖入 Applications；首次设置可选择注册用户级 LaunchAgent，App 不显示 Dock 图标。 |
| Linux | `.deb` | 安装桌面入口和 XDG 登录启动项；首次设置会询问是否启用，提供 amd64、arm64 与 RISC-V 64 包。 |

MSI、DMG 和 DEB launcher 首次运行时都会要求设置管理员用户名、非空密码，并询问是否
随系统登录自动启动。Launcher 会保存用户名，但绝不保存密码。之后可以在内嵌 Console 的
**设置 → 后台运行** 中修改自动启动。关闭只影响下次登录，不会停止当前服务。启动项不会
复制管理员密码、主密钥、DSN 或带认证信息的代理 URL；依赖这些值的部署应继续使用已有
service manager。

后台 launcher 的输出分别写入 Windows 的 `%LOCALAPPDATA%\GPROXY\logs\gproxy.log`
与 `gproxy-error.log`、macOS 的
`~/Library/Logs/GPROXY/gproxy.log`，以及 Linux 的
`${XDG_STATE_HOME:-~/.local/state}/gproxy/gproxy.log`。

## 便携 Release 二进制

如果只想运行 native server 和内嵌 Console，不想在机器上安装 Rust 或 Node，使用
release 二进制。

1. 从[下载页](/zh-cn/getting-started/downloads/)或
   [最新 GitHub Release](https://github.com/LeenHawk/gproxy/releases/latest)
   下载对应 OS 和 CPU 的压缩包。
2. 解压压缩包。Android 上需要把 `gproxy`、`gproxy.bin` 和 `libc++_shared.so` 放在同一目录。
3. 放到 `PATH` 中，或直接运行。

```bash
chmod +x ./gproxy
./gproxy --help
```

release workflow 会构建 Linux、macOS、Windows 和 Android。Linux 覆盖 x86_64、
AArch64 与 RISC-V 64，并同时提供 GNU 和 musl 版本。默认 GNU Docker 镜像与 `-musl`
镜像也都包含这三种 Linux 架构。Android release 还会包含按 ABI 拆分的 APK，适合想
使用可安装包而不是原始 executable 压缩包的用户。

Android APK 包含 launcher UI 和 Foreground Service。安装匹配 ABI 的 APK 后打开
**GPROXY**，填写管理员用户名和密码后点击 **Start GPROXY**。Launcher 中的开关控制
以后打开 App 和设备开机时是否自动启动。开启开关时，App 会说明后台运行用途，并打开
Android 的电池优化权限提示。Service 会用下面的参数运行 native server：

```text
GPROXY_ADMIN_USER=<username>
--host 127.0.0.1 --port 8787 --data-dir <app-private-data>/data
```

密码只传给这一次启动，launcher 不会持久化它。完成首次设置后，密码留空会保持已有
管理员不变；再次填写密码则会主动重置它。Service 运行期间 Android 会显示常驻通知。

Release APK 使用包名 `io.github.leenhawk.gproxy`、app 名称 `GPROXY`，并使用
Console favicon 作为 launcher 图标。正式发布的 APK 必须使用 GitHub Actions 里配置的
Android signing secrets 签名。

然后打开：

```text
http://127.0.0.1:8787/console
```

## Docker 镜像

发布镜像是 `ghcr.io/leenhawk/gproxy`。

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
docker run --rm -p 8787:8787 \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

镜像里已经包含带内嵌 Console 的 native 二进制。镜像默认设置
`GPROXY_HOST=0.0.0.0`、`GPROXY_PORT=8787`、`GPROXY_PERSISTENCE=file`、
`GPROXY_DATA_DIR=/app/data`。

持久化 volume、PostgreSQL/MySQL DSN 和 tag 选择见 [Docker](/zh-cn/deployment/docker/)。

## 从源码构建（仅开发者）

:::caution
这一节只面向 GPROXY 开发和定制构建。如果只是想使用 GPROXY，请直接前往
[下载页](/zh-cn/getting-started/downloads/)，不要为了安装应用而克隆仓库、安装 Rust
和 Node。
:::

仅在开发 GPROXY，或 release 尚未包含目标平台时使用源码构建。

前置条件：

- 支持 edition 2024 的当前 stable Rust 工具链。
- 如果要嵌入当前 `console/` 代码，需要 Node.js 和 pnpm；release workflow 使用
  Node 22 和 pnpm 9。
- 目标平台所需的系统库。

需要嵌入 Console 时，先构建前端：

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..
```

再从仓库根目录构建二进制：

```bash
cargo build --release --bin gproxy
./target/release/gproxy --help
```

`pnpm build` 会生成 `console/dist/`，再运行
`console/scripts/sync-to-embed.mjs` 同步到 `assets/console/`。native 二进制通过
`rust-embed` 编译这个目录。

如果跳过 Console 构建，gateway 和 admin API 仍可编译运行，但 `/console` 可能返回
`console assets not embedded`。

## Edge Bundles

不要让 edge 平台从源码编译 Rust。支持的 edge 路径是上传预构建 bundle：

```text
在有 Rust 的机器/CI 构建 wasm -> 生成平台 bundle -> 上传 bundle
```

release artifacts 包含 `gproxy-edge-cloudflare.zip`、`gproxy-edge-netlify.zip`、
`gproxy-edge-supabase.zip`、`gproxy-edge-deno.zip`、`gproxy-edge-eopages.zip` 和
`gproxy-edge-appwrite-deno.zip`。

平台命令和 runtime secrets 见 [Edge Wasm 部署](/zh-cn/deployment/edge/)。

## 下一步

- 在[下载页](/zh-cn/getting-started/downloads/)选择其他平台或安装包格式。
- 继续 [快速开始](/zh-cn/getting-started/quick-start/)，启动本地实例。
- 反代 native server 前先读 [内嵌 Console](/zh-cn/guides/console/)。
- 把 v2 指向已有 v1 数据目录前，先读 [从 v1 迁移到 v2](/zh-cn/deployment/v1-to-v2/)。
