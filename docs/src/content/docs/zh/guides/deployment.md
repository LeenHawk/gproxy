---
title: 部署
description: 以二进制、Docker 容器或外部数据库方式运行 GPROXY。
---

## 二进制部署

从 [GitHub Releases](https://github.com/LeenHawk/gproxy/releases) 下载对应平台的可执行文件。

```bash
chmod +x gproxy
./gproxy
```

启动即用，GPROXY 自带合理的默认配置：

- 监听 `127.0.0.1:8787`
- 在 `./data/gproxy.db` 创建 SQLite 数据库
- 自动生成管理员账号、密码和 API key（首次启动时打印到日志，请妥善保存）
- 管理控制台地址 `http://127.0.0.1:8787/`

如需自定义管理员凭证：

```bash
./gproxy \
  --admin-user admin \
  --admin-password 'your-password' \
  --admin-api-key 'your-api-key'
```

## Docker

```bash
docker pull ghcr.io/leenhawk/gproxy:latest

docker run --rm -p 8787:8787 \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_PORT=8787 \
  -e GPROXY_ADMIN_USER=admin \
  -e GPROXY_ADMIN_PASSWORD='your-password' \
  -e DATABASE_SECRET_KEY='your-encryption-key' \
  -v $(pwd)/data:/app/data \
  ghcr.io/leenhawk/gproxy:latest
```

### 镜像变体

| Tag | 基础环境 | 适用场景 |
|-----|----------|----------|
| `latest` | glibc | 标准部署 |
| `latest-musl` | musl（静态链接） | Alpine、scratch 或极简容器 |

两种变体均提供 `amd64` 和 `arm64` 架构。

### 持久化存储

将 `/app/data` 挂载为卷。默认 SQLite 数据库和文件数据都存储在此目录下。

```bash
-v $(pwd)/data:/app/data
```

如果使用外部数据库代替 SQLite，设置 `GPROXY_DSN` 即可跳过卷挂载（除非还需要其他基于文件的功能）：

```bash
docker run --rm -p 8787:8787 \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_ADMIN_USER=admin \
  -e GPROXY_ADMIN_PASSWORD='your-password' \
  -e GPROXY_DSN='mysql://user:password@db-host:3306/gproxy' \
  -e DATABASE_SECRET_KEY='your-encryption-key' \
  ghcr.io/leenhawk/gproxy:latest
```

### Docker 环境下的数据库加密

通过环境变量、Docker Secrets 或平台密钥管理服务注入 `DATABASE_SECRET_KEY`。建议在首次启动前就完成设置，确保所有敏感字段从一开始就被加密存储。详情参见[配置参考](/zh/guides/configuration/#database-encryption)。

## 外部数据库

GPROXY 支持 SQLite、MySQL 和 PostgreSQL。通过 `GPROXY_DSN` 设置相应的连接字符串：

```bash
# MySQL
GPROXY_DSN='mysql://user:password@127.0.0.1:3306/gproxy'

# PostgreSQL
GPROXY_DSN='postgres://user:password@127.0.0.1:5432/gproxy'
```

启动时自动同步数据库表结构，无需手动执行迁移。
