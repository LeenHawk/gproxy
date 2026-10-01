---
title: "安装"
description: "选择 GPROXY 4.0 的应用、命令行程序、容器或 Cloudflare Workers 部署。"
---


日常在电脑上使用，选 **Application**；部署到服务器，选 **CLI** 或容器。两者使用相同的网关功能，主要区别是如何启动和管理。

在 [Releases](https://github.com/LeenHawk/gproxy/releases) 选择版本，再下载对应系统和架构的文件。4.0.0 正在准备发布；正式版上线前可使用 `nightly` 试用，`nightly` 会随开发更新，不是稳定版。

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

Windows Release 附件中的 MSIX 是未签名的商店提交包，不能当作已签名安装包直接安装；可选择 ZIP，商店版本以实际商店上架状态为准。macOS 应用使用 ad-hoc 签名，尚未公证。鸿蒙 HAP 需要自行签名，尚未完成真机验证，也不提供后台服务。

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

全新实例会在终端显示生成的管理员密码和 API Key，**只显示一次，请保存**。已有实例不会在重启时重置账户。

打开 **http://127.0.0.1:8787/console/**，使用管理员账户登录。个人页面和管理页面都在这个控制台内。CLI 不使用 Application 的设置向导。

默认只监听本机，SQLite 数据库位于 `./data/gproxy.db`。如果要接受其他设备的连接，需要调整监听地址，并配置网络访问与 HTTPS。主密钥、环境变量和数据库选项见[配置](/zh-cn/reference/configuration/)。

CLI 需要通过 `GPROXY_MASTER_KEY` 配置凭证加密主密钥；未配置时，上游凭证按明文存储，启动日志会提示。请在添加凭证前设置并妥善保存主密钥，后续启动使用同一值。

## 容器

镜像为 `ghcr.io/leenhawk/gproxy`。从所选版本的发布信息取得镜像标签或提交 SHA，替换下面的占位值；不要用旧的 v3 标签启动 v4。

```sh
export GPROXY_IMAGE='ghcr.io/leenhawk/gproxy:<tag-or-commit-sha>'
# GPROXY_MASTER_KEY 应是已保存的 32 字节主密钥（64 位十六进制或 base64）。
docker run -d --name gproxy --restart unless-stopped \
  -p 127.0.0.1:8787:8787 \
  -v gproxy-data:/app/data \
  -e GPROXY_MASTER_KEY \
  "$GPROXY_IMAGE"
docker logs gproxy
```

从首次启动日志保存管理员信息，再打开 `/console/`。镜像以 `65532:65532` 运行；使用宿主机目录挂载时，需要给该用户写权限。数据保存在 `/app/data`，更新容器时保留数据卷。发布流水线构建 amd64、arm64、riscv64 的 GNU 和 musl 镜像。

## Cloudflare Workers

选择 `gproxy-edge-cloudflare.zip`，按[边缘部署](/zh-cn/deployment/edge/)配置数据库和 secrets 后部署。控制台由 Workers Assets 提供；WebSocket / Realtime 使用与原生部署相同的路由。

## 从 v3 升级

先备份数据库、主密钥和启动配置，停止旧进程，再用原配置启动 v4。支持的 v3 SQLite、PostgreSQL、MySQL 和 D1 数据库会自动迁移；请阅读[迁移说明](/zh-cn/deployment/v3-to-v4/)，确认保留的数据和需要检查的迁移报告。

需要自行编译时，见[从源码构建](/zh-cn/deployment/release-build/)。安装完成后继续[快速开始](/zh-cn/getting-started/quick-start/)。
