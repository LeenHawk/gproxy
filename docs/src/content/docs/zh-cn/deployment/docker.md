---
title: Docker 部署
description: 使用容器运行 gproxy，包括持久化数据和通过环境变量传参。
---

仓库自带 [`Dockerfile.action`](https://github.com/LeenHawk/gproxy/blob/main/Dockerfile.action)，
发布流水线用它来构建官方镜像。你也可以在本地构建，或把它当作自己镜像的参考。

## 构建

```bash
docker build -f Dockerfile.action -t gproxy:local .
```

生成的镜像内包含 `gproxy` 二进制 (控制台已内嵌)。容器内不需要再单独构建前端，
前端构建是 Docker 构建流程的一部分。

## 运行

gproxy 需要一个地方持久化数据目录 (使用 SQLite 时就是 SQLite 文件)。
挂载一个 volume，并按常规方式传环境变量：

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
  gproxy:local
```

几点提醒：

- 容器里必须监听 **`0.0.0.0`**，否则容器外无法访问端口。
- **`GPROXY_DATA_DIR`** 指向持久化 volume 内的路径。默认的 `./data` 会落在
  容器工作目录下，容器重建即丢数据。
- **`GPROXY_CONFIG`** 只在首次启动有用；之后 volume 里的数据库是事实来源，
  种子 TOML 会被忽略。

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
  gproxy:local
```

## docker-compose 示例

```yaml
services:
  gproxy:
    image: gproxy:local
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

## 关机行为

`docker stop` 会向主进程发送 `SIGTERM`。gproxy 会像处理 Ctrl+C 一样处理它 ——
Axum drain 在途请求，`UsageSink` 写入最后一批，然后进程退出。给它足够的
宽限时间 (Docker 默认 10 秒即可)。完整流程见
[优雅关机](/zh-cn/reference/graceful-shutdown/)。
