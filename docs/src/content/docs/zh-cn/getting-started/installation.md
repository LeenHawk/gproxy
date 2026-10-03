---
title: "安装"
description: "选择 GPROXY 4.0 的应用、命令行程序、容器或托管平台部署。"
---


日常在电脑上使用，选 **Application**；部署到服务器，选 **CLI** 或容器。两者使用相同的网关功能，主要区别是如何启动和管理。

选择 [最新稳定版](https://github.com/LeenHawk/gproxy/releases/latest)，再下载对应系统和架构的文件。滚动更新的 `nightly` 提供开发快照。

## 选择下载文件

| 平台 | CLI（`gproxy-*`） | Application（`gproxy-tauri-*`） |
| --- | --- | --- |
| Linux GNU：x86_64、aarch64、riscv64 | ZIP、DEB | ZIP、DEB |
| Linux musl：x86_64、aarch64、riscv64 | ZIP、DEB | — |
| Windows：x86_64、aarch64 | ZIP、MSIX | ZIP、MSIX |
| macOS：x86_64、aarch64 | ZIP、DMG | ZIP、DMG |
| Android：x86_64、aarch64 | ZIP、Termux DEB | APK |
| OpenHarmony / HarmonyOS NEXT | ARM64、x86_64 ZIP | ARM64 实验性 HAP |

普通 Intel / AMD 电脑选 x86_64，Apple Silicon 选 aarch64。Linux Application 需要系统提供 WebKitGTK 4.1。

Android CLI 在 Termux 内运行，用 `apt install ./gproxy-android-<architecture>.deb` 安装 DEB。
使用 ZIP 前先执行 `pkg install libc++ openssl ca-certificates`，解压到 Termux 主目录，
并按 `TERMUX.txt` 操作。程序自更新已禁用，通过 APT 安装新版 DEB；启用的软件仓库收录后
才可使用 `pkg upgrade gproxy`。升级时保留原数据目录和主密钥。

Windows（x64、ARM64）可直接从 Microsoft Store 安装两个版本：

| GPROXY Desktop | GPROXY CLI |
| :---: | :---: |
| <a href="https://apps.microsoft.com/detail/9P2FJRB9RS4Z?mode=direct"><img src="https://get.microsoft.com/images/zh-cn%20dark.svg" alt="从 Microsoft 获取：GPROXY Desktop" height="48"></a> | <a href="https://apps.microsoft.com/detail/9NBMH3S5K0L9?mode=direct"><img src="https://get.microsoft.com/images/zh-cn%20dark.svg" alt="从 Microsoft 获取：GPROXY CLI" height="48"></a> |

GPROXY Desktop 即 Application 版本。商店版本由微软签名，并通过商店自动更新。也可以用 `winget install --id 9P2FJRB9RS4Z --source msstore`（Desktop）或 `winget install --id 9NBMH3S5K0L9 --source msstore`（CLI）安装。

Windows Release 附件中的 MSIX 是商店提交包，可能尚未带有微软签名；请从商店安装，或选择 ZIP。macOS 应用使用 ad-hoc 签名，尚未公证。鸿蒙 HAP 需要自行签名，尚未完成真机验证，也不提供后台服务。

## Application：在界面中设置

安装或解压 Application 后，启动 GPROXY。全新实例会打开设置向导：

1. **实例设置**：选择监听地址、端口、数据目录和数据库。初次使用可保留默认值。
2. **管理员账户**：设置用户名和至少 8 个字符的密码。API Key 可自行填写，也可留空生成。
3. **导入配置**：可选导入已有的 v4 配置文件，没有文件时直接完成。

完成后保存页面显示的服务地址和 API Key，再进入控制台。桌面版可选择开机自启和系统托盘；移动版使用应用私有数据目录，具体选项以平台为准。Android 向导还提供通知与后台运行权限入口。

应用窗口通过本机 IPC 管理实例，HTTP 监听端口供客户端调用网关 API。浏览器访问这个端口不会打开应用内向导或管理控制台。

## CLI：启动服务器

解压 CLI ZIP，在终端进入所在目录。Linux / macOS 运行：

```sh
chmod +x ./gproxy
./gproxy serve --data-dir ./data --port 8787
```

Windows 在 PowerShell 中运行：

```powershell
.\gproxy.exe serve --data-dir .\data --port 8787
```

全新实例会在终端显示生成的管理员密码和 API Key，**只显示一次，请保存**。已有实例重启时，显式设置的 `GPROXY_ADMIN_PASSWORD` 优先更新同名用户的密码；没有同名用户时恢复并重命名 0 号管理员；未设置则保留现状。

打开 **http://127.0.0.1:8787/console/**，使用管理员账户登录。个人页面和管理页面都在这个控制台内。CLI 不使用 Application 的设置向导。

默认只监听本机，SQLite 数据库位于 `./data/gproxy.db`。如果要接受其他设备的连接，需要调整监听地址，并配置网络访问与 HTTPS。主密钥、环境变量和数据库选项见[配置](/zh-cn/reference/configuration/)。

CLI 需要通过 `GPROXY_MASTER_KEY` 配置凭证加密主密钥；未配置时，上游凭证按明文存储，启动日志会提示。请在添加凭证前设置并妥善保存主密钥，后续启动使用同一值。

## 容器

发布流水线将镜像同步到 `ghcr.io/leenhawk/gproxy` 和 Docker Hub 的 `leenhawk/gproxy`。稳定版本使用 `vX.Y.Z` 标签，beta 使用 `staging`，开发快照使用 `nightly`；musl 镜像在标签后加 `-musl`。请选择已发布的 v4 标签，不要用旧的 v3 标签启动 v4。

```sh
export GPROXY_IMAGE='leenhawk/gproxy:<tag>'
# GPROXY_MASTER_KEY 应是已保存的 32 字节主密钥（64 位十六进制或 base64）。
docker run -d --name gproxy --restart unless-stopped \
  -p 127.0.0.1:8787:8787 \
  -v gproxy-data:/app/data \
  -e GPROXY_MASTER_KEY \
  "$GPROXY_IMAGE"
docker logs gproxy
```

从首次启动日志保存管理员信息，再打开 `/console/`。镜像以 `65532:65532` 运行；使用宿主机目录挂载时，需要给该用户写权限。数据保存在 `/app/data`，更新容器时保留数据卷。发布流水线构建 amd64、arm64、riscv64 的 GNU 和 musl 镜像。

## 托管平台

Cloudflare Workers、Netlify、Vercel 和 Deno 都提供部署模板，无需本机 Rust 工具链。Cloudflare 可用 D1 或 libSQL/Turso，其他三个平台使用 PostgreSQL；需要 WebSocket / Realtime 时可选 Cloudflare、Vercel（WebSocket Beta）或 Deno。

部署按钮、数据库选择和首次登录步骤统一见[托管平台部署](/zh-cn/deployment/edge/)。需要手动部署 Workers 时，同一页也提供发布包和 Wrangler 的操作步骤。

## 从 v3 升级

先备份数据库、主密钥和启动配置，停止旧进程，再用原配置启动 v4。支持的 v3 SQLite、PostgreSQL、MySQL 和 D1 数据库会自动迁移；请阅读[迁移说明](/zh-cn/deployment/v3-to-v4/)，确认保留的数据和需要检查的迁移报告。

需要自行编译时，见[从源码构建](/zh-cn/deployment/release-build/)。安装完成后继续[快速开始](/zh-cn/getting-started/quick-start/)。
