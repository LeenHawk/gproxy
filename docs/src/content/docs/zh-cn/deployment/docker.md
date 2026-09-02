---
title: "容器部署"
description: "使用 ghcr.io/leenhawk/gproxy 镜像运行，配置持久化 volume、外部数据库、共享缓存、反向代理与 JSON 日志"
---

官方镜像是 `ghcr.io/leenhawk/gproxy:<tag>`，其中 `<tag>` 是 release tag，例如
`v3.0.0-alpha.0`。release workflow 为每个 tag 构建一个 `linux/amd64` 镜像，不再
推送其他内容：没有 `latest` tag，也没有 `-musl` 变体。固定使用你测试过的 tag。
版本列表见[下载](/zh-cn/getting-started/downloads/)。

镜像由多阶段 `deploy/container/Dockerfile` 从源码构建：Node 阶段构建控制台，Rust
阶段嵌入控制台并编译 `gproxy`，运行阶段是只带 `ca-certificates` 的
`debian:trixie-slim`。镜像内的二进制与原生 release 发布的是同一份，只是以安装类型
`container` 编译。

## 镜像默认值

| 设置 | 取值 |
| --- | --- |
| 用户 | `gproxy`（系统用户，home 为 `/var/lib/gproxy`） |
| 端口 | `EXPOSE 8787` |
| 入口 | `/usr/local/bin/gproxy`；额外参数直接传给二进制 |
| `GPROXY_HOST` | `0.0.0.0` |
| `GPROXY_PORT` | `8787` |
| `GPROXY_DATA_DIR` | `/var/lib/gproxy` |
| `GPROXY_PERSISTENCE` | `sqlite` |

默认情况下数据库位于 `/var/lib/gproxy/gproxy.db`。挂载 `/var/lib/gproxy`，否则
数据会随容器一起丢失。命名 volume 会沿用镜像中该目录的所属权；bind 挂载的宿主机
目录必须对 `gproxy` 用户可写。

## 快速运行

```sh
docker run -d --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/var/lib/gproxy \
  ghcr.io/leenhawk/gproxy:v3.0.0-alpha.0
```

打开 `http://127.0.0.1:8787/admin`。全新的 store 会显示初始化表单，用于创建第一个
管理员；之后同一地址就是登录页。若不想通过表单创建管理员，传入首次运行变量：

```sh
docker run -d --name gproxy \
  -p 8787:8787 \
  -v gproxy-data:/var/lib/gproxy \
  -e GPROXY_ADMIN_USER=admin \
  -e GPROXY_ADMIN_PASSWORD='<choose-a-password>' \
  ghcr.io/leenhawk/gproxy:v3.0.0-alpha.0
```

在全新 store 上，这会创建管理员和一个已加密保存的管理员 API 密钥，可在控制台中
查看。`GPROXY_BOOTSTRAP_ADMIN_API_KEY` 可自行指定该密钥，`GPROXY_BOOTSTRAP_CHANNELS`
会按 channel id 各创建一个空 Provider；二者在全新 store 上都需要
`GPROXY_ADMIN_PASSWORD`。只要 `GPROXY_ADMIN_PASSWORD` 仍然设置着，每次启动都会重新
应用该管理员的密码，因此登录成功后请移除它。

二进制还会读取 `<data-dir>/.env` 中的 `GPROXY_*` 键，这里即 volume 内的
`/var/lib/gproxy/.env`；用 `-e` 设置的变量优先。

## Compose

```yaml
services:
  gproxy:
    image: ghcr.io/leenhawk/gproxy:v3.0.0-alpha.0
    restart: unless-stopped
    ports:
      - "8787:8787"
    environment:
      GPROXY_LOG_FORMAT: json
      # GPROXY_MASTER_KEY: "<standard base64, 32 bytes>"
    volumes:
      - gproxy-data:/var/lib/gproxy

volumes:
  gproxy-data:
```

## 落盘加密

把 `GPROXY_MASTER_KEY` 设为标准 base64 编码的 32 字节密钥（`openssl rand -base64 32`），
即可用 AES-256-GCM 加密保存凭证和用户密钥。不设置则以明文保存，数据库本身可信时
这是合适的选择。启动时若缺少加密所用的密钥，已加密的 store 会被拒绝打开。轮换使用
`GPROXY_MASTER_KEY_NEXT` 和 `GPROXY_MASTER_KEY_ROTATE`；见
[配置](/zh-cn/reference/configuration/)。

## 外部数据库与缓存

| 后端 | 变量 |
| --- | --- |
| PostgreSQL | `GPROXY_PERSISTENCE=postgres`、`GPROXY_DSN=postgres://gproxy:<password>@db:5432/gproxy` |
| MySQL | `GPROXY_PERSISTENCE=mysql`、`GPROXY_DSN=mysql://gproxy:<password>@db:3306/gproxy` |
| libSQL / Turso | `GPROXY_PERSISTENCE=libsql`、`GPROXY_LIBSQL_URL=https://<db>-<org>.turso.io`、`GPROXY_LIBSQL_AUTH_TOKEN=<token>` |
| Redis 缓存 | `GPROXY_REDIS_URL=redis://cache:6379`（或 `rediss://`） |
| Upstash 缓存 | `UPSTASH_URL=https://<name>.upstash.io`、`UPSTASH_TOKEN=<token>`；两者同时设置或同时不设 |

PostgreSQL 连接不使用 TLS，请把数据库放在私有网络内。libSQL URL 必须是绝对的
`http(s)` URL；store 通过 HTTP 上的 Hrana 通信，并且在 `libsql` 持久化下，除非
配置了 Redis 或 Upstash，缓存就是一张 libSQL 表。默认缓存是进程内的：运行多个副本
时必须使用 Redis 或 Upstash，配额、限流和 OAuth 刷新租约才能共享。即使使用外部
数据库也要保留 volume；数据目录仍会被创建，并存放 `.env`。见
[存储与缓存后端](/zh-cn/reference/database/)。

```yaml
services:
  gproxy:
    image: ghcr.io/leenhawk/gproxy:v3.0.0-alpha.0
    ports:
      - "8787:8787"
    environment:
      GPROXY_PERSISTENCE: postgres
      GPROXY_DSN: postgres://gproxy:<password>@db:5432/gproxy
      GPROXY_REDIS_URL: redis://cache:6379
    depends_on: [db, cache]
  db:
    image: postgres:17
    environment:
      POSTGRES_USER: gproxy
      POSTGRES_PASSWORD: <password>
      POSTGRES_DB: gproxy
  cache:
    image: redis:7
```

## 反向代理之后

镜像只提供明文 HTTP；在它前面终止 TLS。两个变量告诉网关代理的存在：

| 变量 | 作用 |
| --- | --- |
| `GPROXY_TRUSTED_PROXIES` | 逗号分隔的 IP 地址（不是 CIDR 网段）。当 TCP 对端是 loopback 或列表中的地址时，取 `X-Forwarded-For` 的第一项、否则取 `X-Real-IP` 作为客户端 IP，用于登录限流和审计。来自其他对端时忽略这些 header。 |
| `GPROXY_CORS_ORIGINS` | 允许携带凭据跨站调用 API 的精确浏览器 origin。留空表示仅同源；控制台和门户由网关自身提供时这已足够。 |

在 compose 网络上给代理容器分配固定地址，才能把它列入其中。为 WebSocket 客户端
转发 `Upgrade` 和 `Connection` header，为流式响应关闭响应缓冲，并允许最大 100 MiB
的请求体——这是网关自身的上限。

## 健康检查与版本

没有专门的健康检查端点。两个无需鉴权的请求可以代替：

| 请求 | 含义 |
| --- | --- |
| `GET /build-info.js` | `200`，正文为 `globalThis.__GPROXY_BUILD_INFO__ = {version, channel, buildHash, installationKind}`；进程存活 |
| `GET /admin/api/session` | `200`，正文为 `{"setup_required":false,"user":null}`；数据库可应答 |

`docker run --rm ghcr.io/leenhawk/gproxy:<tag> --version` 打印构建标识。镜像内
没有 `curl` 或 `wget`，因此请从宿主机或编排系统（例如 Kubernetes 的 `httpGet`
探针）探测，而不是在容器内写 `HEALTHCHECK`。

## 日志

二进制把日志写到 stdout。`GPROXY_LOG_FORMAT=json` 把文本切换为按行分隔的 JSON；
`RUST_LOG` 设置过滤级别（默认 `info`）。用 `docker logs -f gproxy` 查看。请求审计
和线上抓包保存在数据库而不是日志中；见[用量、日志与审计](/zh-cn/guides/observability/)。

## 优雅停止

二进制收到 `SIGINT` 或 `SIGTERM` 时会干净地关闭：停止接受连接，并让进行中的请求
完成。镜像声明了 `STOPSIGNAL SIGTERM`，所以普通的 `docker stop gproxy` 会走这条
优雅关闭路径。

## 升级

```sh
docker pull ghcr.io/leenhawk/gproxy:<new-tag>
docker stop gproxy && docker rm gproxy
# 用同一个 volume 和环境变量重新创建容器
```

数据保留在 volume 或外部数据库中。从 v2 容器迁移时，先阅读
[v2 到 v3 迁移](/zh-cn/deployment/v2-to-v3/)；v2 镜像把数据放在 `/app/data`，
而迁移是同一个入口的子命令：

```sh
docker run --rm \
  -v gproxy-data:/var/lib/gproxy \
  -v gproxy-v2-data:/v2:ro \
  ghcr.io/leenhawk/gproxy:v3.0.0-alpha.0 \
  migrate --from-v2 /v2/gproxy.db
```

## 本地构建镜像

`deploy/container/Dockerfile` 在没有 `GPROXY_UPDATE_PUBKEY` 时拒绝构建，且该值必须
恰好解码为 32 字节。构建本地镜像时可生成一把临时密钥：

```sh
PUBKEY="$(openssl genpkey -algorithm ed25519 \
  | openssl pkey -pubout -outform DER | tail -c 32 | base64 -w0)"
docker buildx build \
  -f deploy/container/Dockerfile \
  --build-arg GPROXY_UPDATE_PUBKEY="$PUBKEY" \
  --build-arg GPROXY_BUILD_VERSION=3.0.0-local \
  --build-arg GPROXY_BUILD_CHANNEL=dev \
  --build-arg GPROXY_BUILD_HASH="$(git rev-parse HEAD)" \
  -t gproxy:local .
```

| 构建参数 | 默认值 | 用途 |
| --- | --- | --- |
| `GPROXY_UPDATE_PUBKEY` | 必填 | 编译进二进制的 Ed25519 公钥 |
| `GPROXY_BUILD_VERSION`、`GPROXY_BUILD_HASH` | 未设置 | `--version` 显示的构建标识 |
| `GPROXY_BUILD_CHANNEL` | `releases` | 默认更新 channel |
| `GPROXY_INSTALLATION_KIND` | `container` | `--version` 显示的安装类型 |
| `GPROXY_VERSION`、`GPROXY_REVISION` | 未设置 | OCI 镜像 label |
| `CARGO_NET_OFFLINE` | `false` | 从预热的 cargo 缓存构建 |

构建不需要预先编译控制台；第一阶段会编译它。
`docker buildx build -f deploy/container/Dockerfile --target console-dist --output type=local,dest=dist/console .`
只导出控制台 bundle，release workflow 正是用它为其他所有 job 构建一次控制台。

## 加载 Release 归档

每个 release 还会把推送的镜像发布为文件，供无法访问 registry 的主机使用：

```sh
sha256sum -c gproxy-container-linux-amd64.tar.gz.sha256
docker load -i gproxy-container-linux-amd64.tar.gz
```

加载后的镜像保留原有 tag `ghcr.io/leenhawk/gproxy:<tag>`。旁边的
`gproxy-container-linux-amd64.provenance.json` 记录了提交和基础镜像 digest；见
[构建与发布](/zh-cn/deployment/release-build/)。
