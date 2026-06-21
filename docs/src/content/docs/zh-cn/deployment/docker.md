---
title: Docker
description: 使用发布的 GPROXY v2 Docker 镜像，配置持久化数据和可选数据库 DSN。
---

官方镜像是 `ghcr.io/leenhawk/gproxy`。它是基于预构建 Linux 二进制的轻量 runtime
镜像；Dockerfile 不编译 Rust，也不构建 Console。镜像内的二进制已经包含内嵌
Console 资产。

## Tags

release workflow 先发布各架构镜像，再创建 multi-architecture manifest list。

| Tag | 含义 |
| --- | --- |
| `latest` | 最新发布版本，glibc runtime，amd64 + arm64 manifest。 |
| `<release-tag>` | 指定 release tag。 |
| `latest-musl` | 最新发布版本，static musl runtime，amd64 + arm64 manifest。 |
| `<release-tag>-musl` | 指定 release tag 的 static musl runtime。 |
| `latest-amd64`, `latest-arm64` | glibc 架构镜像，用于 manifest。 |
| `latest-amd64-musl`, `latest-arm64-musl` | musl 架构镜像。 |

大多数部署使用 `latest` 或固定 release tag。

## 快速运行

```bash
docker run --rm \
  --name gproxy \
  -p 8787:8787 \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

打开 <http://127.0.0.1:8787/console>，用 `admin` 和设置的密码登录。

镜像默认设置：

| Env | 镜像默认 |
| --- | --- |
| `GPROXY_HOST` | `0.0.0.0` |
| `GPROXY_PORT` | `8787` |
| `GPROXY_PERSISTENCE` | `file` |
| `GPROXY_DATA_DIR` | `/app/data` |

`file` persistence 是本地磁盘 JSON 存储，适合单容器。需要容器替换后保留数据时，挂载
`/app/data`。

## 持久化 Volume

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/app/data \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

在 Console 中创建持久 admin 密码后，移除 `GPROXY_ADMIN_PASSWORD`。只要它还设置着，
server 每次启动都会强制重置指定 admin 用户。

## First-Boot Import

从 v2 JSON bundle seed providers、routes、credentials、users 和 keys：

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/app/data \
  -v "$PWD/import.json:/etc/gproxy/import.json:ro" \
  -e GPROXY_IMPORT_FILE=/etc/gproxy/import.json \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

import 文件只在 store 为空时使用。已有 users 或 providers 后，后续启动会跳过。

## SQLite、PostgreSQL 或 MySQL

native 二进制默认 `db` persistence，但 Docker 镜像默认 `file`。需要数据库 backend 时，
显式设置 `GPROXY_PERSISTENCE=db`。

挂载 data 目录中的 SQLite：

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/app/data \
  -e GPROXY_PERSISTENCE=db \
  -e GPROXY_DATA_DIR=/app/data \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

`db` persistence 且未设置 `GPROXY_DSN` 时，v2 会派生
`sqlite://<data-dir>/gproxy.db?mode=rwc`。

PostgreSQL 示例：

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -e GPROXY_PERSISTENCE=db \
  -e GPROXY_DSN='postgres://gproxy:secret@postgres.internal:5432/gproxy' \
  -e GPROXY_MASTER_KEY="$GPROXY_MASTER_KEY" \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

需要 sealed secrets 时，`GPROXY_MASTER_KEY` 必须是标准 base64 编码的 32 字节。

## docker compose

```yaml
services:
  gproxy:
    image: ghcr.io/leenhawk/gproxy:latest
    restart: unless-stopped
    ports:
      - "8787:8787"
    environment:
      GPROXY_ADMIN_PASSWORD: change-me-please
      GPROXY_MASTER_KEY: ${GPROXY_MASTER_KEY:-}
    volumes:
      - gproxy-data:/app/data

volumes:
  gproxy-data:
```

## 升级

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
docker stop gproxy
docker rm gproxy
# 使用同一个 volume 和环境变量重新创建容器
```

数据保留在挂载 volume 或外部数据库中。如果要从 v1 迁移，先阅读
[从 v1 迁移到 v2](/zh-cn/deployment/v1-to-v2/)，再让 v2 容器指向旧 SQLite 文件。
