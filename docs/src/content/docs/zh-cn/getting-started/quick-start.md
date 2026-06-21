---
title: 快速开始
description: 本地启动 gproxy v2，导入最小配置 bundle，打开 Console，并发出第一条请求。
---

这页会启动一个带内嵌 Console 的本地 native gproxy v2 实例，并导入一个最小 JSON
bundle。这里使用当前 v2 配置模型：运行时设置来自 CLI flag 或环境变量；provider、
route、user、key、rule 等控制面记录存在 persistence 中，可通过 JSON 导入。

## 1. 构建或安装

可以使用 release 二进制、Docker 镜像或本地源码构建。源码构建并嵌入 Console：

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..

cargo build --release --bin gproxy
```

二进制位于 `target/release/gproxy`。

## 2. 准备开发导入 bundle

docs site 带有开发 bundle：`docs/public/examples/import-dev.json`。写入真实上游 key
前，先复制到 docs 目录外：

```bash
cp docs/public/examples/import-dev.json ./import-dev.local.json
```

编辑复制后的文件并替换：

- `sk-REPLACE`：OpenAI-compatible 上游 key。
- `sk-ant-REPLACE`：Anthropic-compatible 上游 key；仅测试 Claude provider 时需要。

示例 bundle 会创建：

- org `default`；
- admin user `dev`；
- user API key `sk-dev-local`；
- provider `openai-main`；
- route `main`，指向上游模型 `gpt-4.1-mini`；
- default org 的 wildcard route permission。

:::caution
`import-dev.local.json` 包含明文上游凭证和 user API key。只放本地，不要提交。
:::

## 3. 启动 gproxy

用本地 data 目录启动 native 二进制，并让 first-boot hook 在空 store 时导入 bundle：

```bash
GPROXY_DATA_DIR=./data \
GPROXY_IMPORT_FILE=./import-dev.local.json \
GPROXY_ADMIN_USER=dev \
GPROXY_ADMIN_PASSWORD=change-me-please \
./target/release/gproxy
```

常用默认值：

| 设置 | 默认值 |
| --- | --- |
| `GPROXY_HOST` | `127.0.0.1` |
| `GPROXY_PORT` | `8787` |
| `GPROXY_PERSISTENCE` | `db` |
| `GPROXY_DATA_DIR` | `./data` |
| `GPROXY_DSN` | 未设置时为 `<data-dir>/gproxy.db` SQLite |

`GPROXY_IMPORT_FILE` 只在 providers 和 users 都为空时导入。store 已有数据后，这个环境变量会被跳过。

`GPROXY_ADMIN_USER=dev` 让恢复覆盖开关指向导入 bundle 中的 admin 用户。只要设置着
`GPROXY_ADMIN_PASSWORD`，gproxy 每次启动都会强制 upsert 这个 admin 用户。首次登录后如果
不希望宿主机继续拥有重置密码路径，应移除它。

如需加密保存 secret，设置 `GPROXY_MASTER_KEY`，值必须是标准 base64 编码的 32 字节。
不设置时，v2 使用明文 secret 模式并输出 warning。

## 4. 打开 Console

打开 <http://127.0.0.1:8787/console>。

开发 bundle 的用户是 `dev`，上面的启动命令会把 admin 密码设置为
`change-me-please`。Console 中可查看和管理 providers、credentials、routes、route
members、route permissions、rate limits、quotas、usage、logs 和 update settings。

## 5. 发起 gateway 请求

使用导入的 user key：

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "messages": [
      { "role": "user", "content": "Say hello in one short sentence." }
    ]
  }'
```

聚合入口 `/v1` 会把 `main` 解析成 v2 route。选中的 route member 会在转发前把上游模型改写为
`gpt-4.1-mini`。

provider scoped 请求使用 `/{provider}/v1/...`：

```bash
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [
      { "role": "user", "content": "Say hello in one short sentence." }
    ]
  }'
```

继续阅读 [第一条请求](/zh-cn/getting-started/first-request/)，了解这两种路径背后的路由规则。
