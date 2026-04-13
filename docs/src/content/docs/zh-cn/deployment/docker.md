---
title: Docker 部署
description: 使用 GHCR 官方预构建镜像在容器里运行 GPROXY。
---

GPROXY 官方容器镜像由发布流水线推送到 GitHub Container Registry：
**`ghcr.io/leenhawk/gproxy`**。**直接拉取就行，不用自己构建**，除非你在给 GPROXY 本身改代码。

## 镜像 tag

| Tag | 什么时候更新 | 说明 |
|---|---|---|
| `latest` | 每次发版 | 稳定版，多架构 (amd64 + arm64)，glibc base。**绝大多数用户用这个**。 |
| `v1.2.3` | 发版打 tag 时 | 固定版本号，方便可复现部署。 |
| `staging` | 每次推送到 `main` | 来自 `main` 分支的最新预发布，多架构 glibc。只有想尝鲜修复的情况才用。 |
| `latest-musl` / `v1.2.3-musl` / `staging-musl` | 对应上述 tag 的 musl 版本 | 静态 musl 构建，runtime base 更小，不依赖 glibc。 |

按架构细分的 tag (`latest-amd64`、`latest-arm64`、加 `-musl` 后缀的各变体) 也存在，但不加后缀的就是多架构 manifest list，Docker 会自动挑到对的架构 —— 正常情况**直接用短 tag** 就好。

## 拉取

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
```

镜像是公开的，不需要登录。

## 运行

GPROXY 需要一个地方持久化数据目录 (使用 SQLite 时就是 SQLite 文件)。挂载一个 volume，并按常规方式传环境变量：

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/var/lib/gproxy \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_PORT=8787 \
  -e GPROXY_DATA_DIR=/var/lib/gproxy \
  -e GPROXY_CONFIG=/etc/gproxy/seed.toml \
  -e GPROXY_ADMIN_USER=admin \
  -e GPROXY_ADMIN_PASSWORD=change-me \
  -v "$PWD/seed.toml:/etc/gproxy/seed.toml:ro" \
  ghcr.io/leenhawk/gproxy:latest
```

几点提醒：

- 容器里必须监听 **`0.0.0.0`**，否则容器外无法访问端口。
- **`GPROXY_DATA_DIR`** 指向持久化 volume 内的路径。默认的 `./data` 会落在容器工作目录下，容器重建即丢数据。
- **`GPROXY_CONFIG`** 只在首次启动有用；之后 volume 里的数据库是事实来源，种子 TOML 会被忽略。

## 配合 PostgreSQL

让 `GPROXY_DSN` 指向数据库，就可以省掉 SQLite 持久化 volume：

```bash
docker run -d \
  --name gproxy \
  -p 8787:8787 \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_DSN=postgres://gproxy:secret@postgres.internal:5432/gproxy \
  -e DATABASE_SECRET_KEY=$(cat gproxy-db-key) \
  -e GPROXY_ADMIN_USER=admin \
  -e GPROXY_ADMIN_PASSWORD=change-me \
  ghcr.io/leenhawk/gproxy:latest
```

## docker-compose 示例

```yaml
services:
  gproxy:
    image: ghcr.io/leenhawk/gproxy:latest
    restart: unless-stopped
    ports:
      - "8787:8787"
    environment:
      GPROXY_HOST: 0.0.0.0
      GPROXY_PORT: "8787"
      GPROXY_DATA_DIR: /var/lib/gproxy
      GPROXY_CONFIG: /etc/gproxy/seed.toml
      GPROXY_ADMIN_USER: admin
      GPROXY_ADMIN_PASSWORD: change-me
    volumes:
      - gproxy-data:/var/lib/gproxy
      - ./seed.toml:/etc/gproxy/seed.toml:ro

volumes:
  gproxy-data:
```

## 升级

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
docker stop gproxy && docker rm gproxy
# 重新执行上面的 docker run 命令
```

数据 volume 在容器替换时保留，所以数据库、凭据、日志都会延续到新容器里。如果你钉了某个版本号 (`v1.2.3`)，把 pull / run 里的 tag 改成新版本号即可。

## 关机行为

`docker stop` 会向主进程发送 `SIGTERM`。GPROXY 会像处理 Ctrl+C 一样处理它 —— Axum drain 在途请求，`UsageSink` 写入最后一批，然后进程退出。给它足够的宽限时间 (Docker 默认 10 秒即可)。完整流程见 [优雅关机](/zh-cn/reference/graceful-shutdown/)。

## 自行构建 (仅贡献者)

如果你在给 GPROXY 改代码，想本地验证 Dockerfile 的改动，仓库里有 [`Dockerfile.action`](https://github.com/LeenHawk/gproxy/blob/main/Dockerfile.action) —— 发布流水线用的就是这份。本地构建命令：

```bash
docker build -f Dockerfile.action -t gproxy:dev .
```

普通用户不需要这一步 —— 直接拉 `ghcr.io/leenhawk/gproxy:latest` 即可。
