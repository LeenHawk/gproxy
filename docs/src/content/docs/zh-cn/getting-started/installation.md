---
title: 安装
description: 安装各类 GPROXY 安装包，找到数据目录与日志，完成首次启动，更新、管理登录启动并卸载。
---

GPROXY 是一个可执行文件 `gproxy`，内嵌控制台、门户和公开站点。[下载](/zh-cn/getting-started/downloads/)
页上的每种安装包都包含这个可执行文件，区别只在启动方式和数据位置。

## 安装包

### Linux `.deb`

```bash
sudo apt install ./gproxy-linux-x86_64.deb
```

安装 `/usr/bin/gproxy`、启动器 `/usr/bin/gproxy-desktop`、名为 GPROXY 的应用菜单
项，以及 `/etc/xdg/autostart/gproxy.desktop`，让启动器在登录时运行。依赖
`curl`、`xdg-utils` 和 `zenity`。启动器会创建
`${XDG_DATA_HOME:-~/.local/share}/gproxy`，首次运行时在其中写入私有 `.env`；当
`http://127.0.0.1:8787/admin` 无响应时从该目录启动 `gproxy`，然后打开控制台。如果
你自己在终端运行 `gproxy`，它会使用当前目录下的 `./data`——那是另一个实例、另一份
数据。

### macOS `.dmg`

把 `GPROXY.app` 拖到 Applications 并打开。应用从
`~/Library/Application Support/GPROXY` 运行服务，首次运行时在其中写入私有
`.env`，注册 `~/Library/LaunchAgents/io.github.leenhawk.gproxy.plist` 以便登录时
启动并保持存活，然后打开控制台。它没有 Dock 图标。应用包是 ad-hoc 签名，未经
公证，要求 macOS 11。

### Windows `.msi`

安装包按用户安装，不需要管理员权限。它把 `gproxy.exe` 和启动脚本放到
`%LOCALAPPDATA%\Programs\GPROXY`，创建一个按需启动服务并打开控制台的开始菜单快捷
方式，以及一个在登录时隐藏启动服务的启动文件夹快捷方式。启动器在
`%LOCALAPPDATA%\GPROXY` 下工作，首次运行时在那里写入私有 `.env`。

### Android `.apk`

包名为 `io.github.leenhawk.gproxy`，最低要求 Android 9（API 28）。允许安装未知来
源应用，安装对应 ABI 的 APK 并打开 GPROXY。界面上有一个"Start automatically on
app launch and device boot"开关（默认开启）、启动服务、打开控制台和停止的按钮，以
及日志视图。自动启动开启时，应用会请求忽略电池优化。服务把二进制复制到应用私有存
储，写入私有 `.env`，监听 `127.0.0.1:8787`，运行期间显示常驻通知。

### 便携压缩包

```bash
unzip gproxy-linux-x86_64.zip -d gproxy && cd gproxy
chmod +x ./gproxy
./gproxy
```

除非用 `--data-dir` 或 `GPROXY_DATA_DIR` 另行指定，数据写入当前目录下的
`./data`。Android 上把 `gproxy`、`gproxy.bin` 和 `libc++_shared.so` 放在一起，运行
`./gproxy`。

### 容器

```bash
docker run -d --name gproxy -p 8787:8787 \
  -v gproxy-data:/var/lib/gproxy \
  ghcr.io/leenhawk/gproxy:<tag>
```

镜像设置 `GPROXY_HOST=0.0.0.0`、`GPROXY_PORT=8787`、
`GPROXY_DATA_DIR=/var/lib/gproxy` 和 `GPROXY_PERSISTENCE=sqlite`，以非特权用户
`gproxy` 运行，暴露 8787 端口。请挂载 `/var/lib/gproxy`。见[容器部署](/zh-cn/deployment/docker/)。

### Edge

Edge Bundle 用各平台自己的工具部署，需要一个 libSQL 数据库。见
[Edge Wasm](/zh-cn/deployment/edge/)。

## 数据在哪里

| 安装包 | 数据目录 | 日志 |
| --- | --- | --- |
| 便携版 | 工作目录下的 `./data` | 标准输出与标准错误 |
| `.deb` 启动器 | `${XDG_DATA_HOME:-~/.local/share}/gproxy` | `${XDG_STATE_HOME:-~/.local/state}/gproxy/gproxy.log` |
| `.dmg` | `~/Library/Application Support/GPROXY` | `~/Library/Logs/GPROXY/gproxy.log` |
| `.msi` | `%LOCALAPPDATA%\GPROXY\data` | `%LOCALAPPDATA%\GPROXY\logs\gproxy.log` 与 `gproxy-error.log` |
| `.apk` | 应用私有存储中的 `files/data` | 应用内的日志视图 |
| 容器 | `/var/lib/gproxy` | 容器标准输出 |

数据目录保存 `gproxy.db`（SQLite 存储）、`.autostart-initialized` 标记，以及更新
暂存期间的 `.update/`。使用 `libsql`、`postgres` 或 `mysql` 后端时，这个目录仍然
存在，用于存放这些文件。

配置按层叠加：命令行、进程环境、工作目录下的 `./.env`、`<数据目录>/.env`、默认
值。`.env` 中只读取 `GPROXY_*` 键以及 `UPSTASH_URL` 和 `UPSTASH_TOKEN`，其他键被
忽略。完整列表见 `gproxy --help` 和[配置](/zh-cn/reference/configuration/)。

:::caution[保管好生成的主密钥]
每种安装包和 Android 应用都会在各自的私有 `.env` 中写入一个随机的
`GPROXY_MASTER_KEY`（`.deb` 和 `.dmg` 启动器还会写入 `GPROXY_DATA_DIR=.`）。此后
保存的凭证和用户密钥都用它加密，没有它 GPROXY 会拒绝打开已加密的数据库。请把
`.env` 和 `gproxy.db` 一起备份。便携版在你自己设置这个变量之前以明文保存密钥。
:::

## 首次启动

打开 `http://127.0.0.1:8787/admin`。存储中还没有管理员时，控制台显示**创建管理
员**；填写用户名和密码即完成登录。管理员是带管理标志的用户；同一账户用于登录控制
台，它的 API 密钥就是普通的用户密钥。

无人值守部署可以从环境变量创建管理员：

```env
GPROXY_ADMIN_USER=admin
GPROXY_ADMIN_PASSWORD=<强密码>
# GPROXY_BOOTSTRAP_ADMIN_API_KEY=sk-<your-key>
# GPROXY_BOOTSTRAP_CHANNELS=openai,claudeapi
```

在全新存储上，这会创建管理员并为其签发一个 API 密钥（使用提供的值，或生成一个，
之后可在身份页通过显示密钥操作读取），并为列出的每个通道 id 创建一个已启用的
Provider，名称与通道相同，带该通道的路由默认值和一个空的私有规则集，暂无凭证。引导密钥和通道需要
`GPROXY_ADMIN_PASSWORD`，且一旦已有管理员就会被忽略。在已有数据的存储上，
`GPROXY_ADMIN_PASSWORD` 会重置 `GPROXY_ADMIN_USER` 所指管理员的密码（该账户存在
时），否则不做任何事；因此首次启动后请移除它，除非确实要重置密码。

## 更新

原生安装在控制台的**更新**页更新。更新通道（**构建默认**、**Dev（alpha）**、
**稳定版**、**预发布版**）和自动检查开关（默认关闭）是实例设置，所有管理员共
享。**检查更新**获取该通道的清单，显示已安装与最新版本、解析出的通道、构建目标、
重启模式和发布说明。

**验证并应用**先下载清单，只有当 Ed25519 签名有效、通道匹配、清单中含有当前运行的
目标三元组、且最低数据版本不高于此二进制的模式版本时才接受。随后下载压缩包，校验
大小和 SHA-256，把可执行文件暂存到 `<数据目录>/.update/`，把正在运行的可执行文件
复制为 `<exe>.prev`，再完成替换。**回滚**恢复 `<exe>.prev`。Android 上会暂存经过
验证的 APK 并打开系统安装器；应用需要"安装未知应用"权限。

| 变量 | 作用 |
| --- | --- |
| `GPROXY_UPDATE_CHANNEL` | 覆盖已保存的通道：`releases`、`staging` 或 `dev`。 |
| `GPROXY_UPDATE_CHANNEL_SERVE` | 同上，且优先于 `GPROXY_UPDATE_CHANNEL`。 |
| `GPROXY_UPDATE_SERVE` | 用于替代 GitHub 的清单 URL，适合镜像。 |
| `GPROXY_UPDATE_RESTART` | `none`（默认）：由你自行重启进程。`supervisor`：应用或回滚后以退出码 42 退出，交给进程管理器重启。`re-exec`：原地重新执行新二进制（Windows 上以 42 退出）。 |

设置了 `GPROXY_UPSTREAM_PROXY_URL` 时更新器会使用它。`releases` 和 `dev` 按语义化
版本比较；`staging` 按构建哈希比较。

## 登录时启动

**设置 → 登录时启动**管理由二进制自身写入的按用户自动启动项。在某个数据目录首次
启动时会创建该项，除非设置了 `GPROXY_AUTOSTART=off`；决定记录在
`.autostart-initialized` 中。Linux 需要桌面会话（`DISPLAY`、`WAYLAND_DISPLAY` 或
`XDG_CURRENT_DESKTOP`），容器内跳过。

| 平台 | 启动项 |
| --- | --- |
| Linux | `~/.config/autostart/gproxy.desktop`（遵循 `XDG_CONFIG_HOME`） |
| macOS | `~/Library/LaunchAgents/io.github.leenhawk.gproxy.plist` |
| Windows | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`，值名 `GPROXY` |

启动项记录可执行文件、启动时使用的参数、工作目录，以及在设置了 `GPROXY_MASTER_KEY`
时复制进去的 `--master-key`，因此要按含密信息对待。关闭开关会删除启动项，但不会停
止正在运行的服务。`.deb` 的自动启动文件、`.msi` 的启动文件夹快捷方式和 `.dmg` 的
LaunchAgent 属于安装包的启动器，与此开关相互独立。

## 卸载

- `.deb`：`sudo apt remove gproxy`。数据目录、日志目录和按用户的
  `~/.config/autostart/gproxy.desktop` 会保留。
- `.dmg`：删除 `~/Library/LaunchAgents/io.github.leenhawk.gproxy.plist`，再把
  `GPROXY.app` 移到废纸篓。数据和日志目录会保留。
- `.msi`：在 Windows 应用设置中移除 GPROXY。`%LOCALAPPDATA%\GPROXY` 以及已启用时的
  `GPROXY` Run 值会保留。
- `.apk`：卸载应用；Android 会连同私有存储一起删除。
- 容器：`docker rm gproxy`；卷会保留到你删除它为止。
- 便携版：删除可执行文件、`<exe>.prev` 和数据目录。

## 下一步

- [快速开始](/zh-cn/getting-started/quick-start/)，配置第一条路由。
- [配置](/zh-cn/reference/configuration/)，查阅全部参数和变量。
- 把实例暴露到 localhost 之外前，阅读[控制台、门户与公开站点](/zh-cn/guides/console/)。
