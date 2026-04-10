---
title: 部署
description: GPROXY 部署说明：本地（二进制、Docker）与云端（ClawCloud Run）。
---

## 本地部署

### 二进制

1. 从 [GitHub Releases](https://github.com/LeenHawk/gproxy/releases) 下载对应平台二进制。
2. 准备配置文件：

```bash
cp gproxy.example.toml gproxy.toml
```

3. 启动服务：

```bash
./gproxy
```

启动后可访问：

- 管理端：`http://127.0.0.1:8787/`

### Docker

拉取预构建镜像（推荐）：

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
```

运行容器：

```bash
docker run --rm -p 8787:8787 \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_PORT=8787 \
  -e GPROXY_ADMIN_KEY=your-admin-key \
  -e DATABASE_SECRET_KEY='replace-with-long-random-string' \
  -e GPROXY_DSN='sqlite:///app/data/gproxy.db?mode=rwc' \
  -v $(pwd)/data:/app/data \
  ghcr.io/leenhawk/gproxy:latest
```

> 建议通过 Docker Secret、平台 Secret 或环境变量注入 `DATABASE_SECRET_KEY`。尤其是使用免费额度或共享型托管数据库时，尽量在首次初始化前就配置好，避免敏感字段明文落库。

## 云端部署

### ClawCloud Run

当前云端模板提供 ClawCloud Run。

- 模板文件：[`claw.yaml`](https://github.com/LeenHawk/gproxy/blob/main/claw.yaml)
- 预构建镜像：`ghcr.io/leenhawk/gproxy:latest`
- 可在 ClawCloud Run 的 App Store -> My Apps -> Debugging 中使用该模板

推荐输入项：

- `admin_key`（默认自动生成随机值）
- `rust_log`（默认 `info`）
- `volume_size`（默认 `1`）
- 通过平台 Secret 配置 `DATABASE_SECRET_KEY`
- 将 `/app/data` 挂载为持久化卷

内置环境变量默认值：

- `GPROXY_HOST=0.0.0.0`
- `GPROXY_PORT=8787`
- `GPROXY_DSN=sqlite:///app/data/gproxy.db?mode=rwc`

可选输入项：

- `proxy_url`（上游代理）

### Release 下载与自更新（Cloudflare Pages）

- 发布工作流还会部署一个独立的 Cloudflare Pages 下载站，用于托管二进制和更新清单。
- 默认公开地址：`https://download-gproxy.leenhawk.com`
- 会生成以下清单：
  - `/manifest.json` —— 文档下载页使用的全量文件索引
  - `/releases/manifest.json` —— 正式版自更新通道
  - `/staging/manifest.json` —— 预览版自更新通道
- 管理后台中的 `Cloudflare 源` 会从该站点读取更新。
- 下载站部署所需仓库 secrets：
  - `CLOUDFLARE_API_TOKEN`
  - `CLOUDFLARE_ACCOUNT_ID`
  - `CLOUDFLARE_DOWNLOADS_PROJECT_NAME`
- 可选仓库 secrets：
  - `DOWNLOAD_PUBLIC_BASE_URL`
  - `UPDATE_SIGNING_KEY_ID`
  - `UPDATE_SIGNING_PRIVATE_KEY_B64`
  - `UPDATE_SIGNING_PUBLIC_KEY_B64`
