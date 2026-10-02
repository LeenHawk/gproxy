---
title: "托管平台部署"
description: "在 Cloudflare Workers、Netlify、Vercel 或 Deno 上部署 GPROXY，选择数据库并完成首次登录。"
---

GPROXY 提供 Cloudflare Workers、Netlify、Vercel 和 Deno 的部署模板。模板下载预编译的发布包，不需要安装 Rust。部署后，控制台和 API 使用同一个域名，控制台入口为 `/console/`。

## 选择平台

| 平台 | 使用平台数据库 | 使用已有数据库 | WebSocket / Realtime |
| --- | --- | --- | --- |
| [Cloudflare Workers](#cloudflare-workers) | 部署时创建 D1 | libSQL / Turso | 支持 |
| [Netlify Functions](#netlify) | Netlify Database | PostgreSQL | 不支持 |
| [Vercel Functions](#vercel) | 部署时选择 PostgreSQL 产品 | PostgreSQL | 不支持 |
| [Deno Deploy](#deno) | 创建应用后绑定 PostgreSQL | PostgreSQL | 不支持 |

只需要 HTTP 和 SSE 流式调用时，可选用任一模板；需要 WebSocket 或 Realtime 时，选择 Cloudflare Workers，或使用 [CLI / 容器部署](/zh-cn/getting-started/installation/)。

Cloudflare 使用 WASM，另外三个平台使用原生 serverless 程序。它们共用 GPROXY 的管理 API 和控制台，但数据库类型、函数时长和请求大小限制取决于平台。

## 准备密码与主密钥

每个平台都需要填写以下两项：

| 配置 | 填写内容 |
| --- | --- |
| `GPROXY_ADMIN_PASSWORD` | 至少 8 个字符的登录密码 |
| `GPROXY_MASTER_KEY` | 用于加密上游凭证的 32 字节主密钥；支持 64 位十六进制或 base64 |

可以用 `openssl rand -hex 32` 生成主密钥。保存好这个值，更新和重新部署时继续使用它。

用户名默认为 `admin`，可通过 `GPROXY_ADMIN_USER` 指定。密码配置会在启动时应用：优先更新同名用户的密码；没有同名用户时恢复 0 号管理员并修改用户名和密码。详见[管理员初始化与密码覆盖](/zh-cn/reference/configuration/#首次初始化)。

使用已有数据库时，还需要数据库连接信息。各平台的填写方式如下。

## Cloudflare Workers

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)][cloudflare-auto]

[使用外部 libSQL / Turso 数据库][cloudflare-external]

使用 D1 点部署按钮，已有 libSQL/Turso 数据库则点外部数据库链接。

1. 进入所选部署入口，连接 GitHub 或 GitLab 账户。
2. 使用 D1 时，填写数据库名称，Cloudflare 会创建并绑定它。使用外部数据库时，填写 `GPROXY_DATABASE_URL` 和 `GPROXY_LIBSQL_TOKEN`；这个入口不会创建 D1。
3. 填写管理员密码和主密钥，保留 `npm run build`、`npm run deploy` 命令，然后部署。

Turso 地址可使用 `libsql://your-db.turso.io` 或对应的 HTTPS 地址。这里使用 libSQL 的 HTTP 接口，不能填写 PostgreSQL 连接串。

已有 D1 部署也可以通过上述两个变量改用 libSQL。设置 `GPROXY_DATABASE_URL` 后，它优先于 D1；更改连接只切换数据库，不会搬迁原有数据。

需要使用 Wrangler 或调整 Workers 配置时，见本页的[手动部署 Cloudflare](#手动部署-cloudflare)。

## Netlify

[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)][netlify-auto]

[使用已有 PostgreSQL 数据库][netlify-external]

使用 Netlify Database 点部署按钮，已有 PostgreSQL 则点外部数据库链接。

1. 进入所选部署入口，授权仓库，并填写管理员密码和主密钥。
2. 使用 Netlify Database 时，模板读取平台提供的连接，无需手填连接串。使用已有数据库时，在 `GPROXY_DATABASE_URL` 中填写 PostgreSQL 连接串。
3. 保留模板的构建命令和 Functions 配置，开始部署。

外部连接的格式例如 `postgresql://user:password@db.example.com/gproxy?sslmode=require`，请替换为数据库服务提供的实际连接信息。

两个入口使用同一个 Netlify 模板。即使填写外部连接，Netlify 仍可能创建平台数据库；GPROXY 会优先使用 `GPROXY_DATABASE_URL`。

## Vercel

[![Deploy with Vercel](https://vercel.com/button)][vercel-auto]

[使用已有 PostgreSQL 数据库][vercel-external]

需要新数据库时点部署按钮，已有 PostgreSQL 则点外部数据库链接。

1. 进入所选部署入口。使用平台数据库时，按页面提示选择并关联 PostgreSQL 产品；外部数据库入口不要求安装数据库产品。
2. 填写管理员密码和主密钥。使用已有数据库时，再填写 `GPROXY_DATABASE_URL`。
3. 保留模板的构建命令和 `vercel.json`，开始部署。

平台数据库连接从 `DATABASE_URL` 或 `POSTGRES_URL` 读取。如果所选产品使用其他变量名，把连接串填入 `GPROXY_DATABASE_URL`。

这个模板使用 **Node.js Functions**，部署时不要改为 Edge Runtime。

## Deno

[在 Deno Deploy 创建应用][deno]

使用新版 Deno Deploy（`console.deno.com`）。创建应用时选择包含模板的 `dev` 分支，确认使用 `deploy/serverless` 目录中的 `deno.json`。

1. 填写管理员密码和主密钥。
2. 已有 PostgreSQL 时，填写 `GPROXY_DATABASE_URL`。需要平台数据库时，创建应用后进入 **Databases** 页面，创建或绑定 PostgreSQL 数据库。
3. 完成数据库配置后重新部署。平台绑定的连接由 `DATABASE_URL` 提供。

Deno 的入口不会自动创建数据库。绑定完成前应用可能返回 503；补齐配置后重新部署即可。

## 首次登录与调用

部署完成后，打开 `https://你的域名/console/`，使用 `admin`（或自定义用户名）和配置的密码登录。

在控制台添加供应商与凭证，测试成功后创建模型路由和网关 API Key，再按[第一个请求](/zh-cn/getting-started/first-request/)发起调用。客户端使用部署域名作为服务地址，例如 `https://gateway.example.com/v1`，使用网关 API Key 鉴权。

如果 Netlify、Vercel 或 Deno 返回 503，先查看函数日志，确认数据库已绑定、连接串有效，以及管理员密码和主密钥已设置。这些平台的 PostgreSQL 账户还需要创建 `gproxy` schema 和应用表的权限。

## 更新与运行限制

配置、账户和用量保存在数据库中，更新时保留数据库和主密钥。模板的 `prepare-release.mjs` 固定发布版本，当前为 `v4.0.2`；升级时修改版本并重新部署，使用的版本需包含对应平台的发布附件。托管部署不使用控制台的原地二进制更新。

Netlify、Vercel 和 Deno 的模板支持 HTTP 与 SSE，不接受 WebSocket 升级。长时间推理、大文件和高并发请求仍受平台限制，部署前可查看 [Netlify Functions](https://docs.netlify.com/build/functions/overview/)、[Vercel Functions](https://vercel.com/docs/functions/limitations) 和 [Deno Deploy](https://docs.deno.com/deploy/reference/limits/) 的当前额度。

这三个模板不配置文件存储。需要 S3/R2 或可公开下载的文件时，可以使用下方的 Workers 自定义构建，或选择 CLI 部署。Netlify、Vercel 和 Deno 可用 `GPROXY_PUBLIC_BASE_URL` 固定公开访问地址；不设置时从请求地址推导。

## 手动部署 Cloudflare

### 使用发布包

下载 `gproxy-edge-cloudflare.zip`，解压并进入 `cloudflare` 目录。包内包含 Worker、控制台和 `wrangler.toml`。

使用 D1 时，安装依赖并创建数据库：

```sh
pnpm install
pnpm exec wrangler d1 create gproxy
```

把返回的数据库 ID 填入 `wrangler.toml`，保留 binding 名 `DB`：

```toml
[[d1_databases]]
binding = "DB"
database_name = "gproxy"
database_id = "replace-with-your-database-id"
```

设置主密钥和管理员密码，再部署：

```sh
pnpm exec wrangler secret put GPROXY_MASTER_KEY
pnpm exec wrangler secret put GPROXY_ADMIN_PASSWORD
pnpm exec wrangler deploy --dry-run
pnpm exec wrangler deploy
```

使用 libSQL 时，设置 `GPROXY_DATABASE_URL` 和 `GPROXY_LIBSQL_TOKEN`，并移除不使用的 D1 binding。发布包已启用 libSQL；自行构建时需保留对应 feature。数据库表由 GPROXY 首次启动时创建，无需单独执行 Wrangler SQL migrations。

### Workers 配置与静态资源

`GPROXY_CONFIG` 接受 JSON 配置，具名 Secrets 覆盖对应字段。以下为 D1 与公开访问地址的配置示例：

```toml
[vars]
GPROXY_CONFIG = '{"store":{"kind":"d1","binding":"DB"},"cache":{"kind":"store"},"public_base_url":"https://gateway.example.workers.dev"}'
```

控制台由 Workers Assets 提供。保留模板中的路由配置，让 `/console/` 使用静态资源，其他请求进入网关：

```toml
[assets]
directory = "./public"
binding = "ASSETS"
not_found_handling = "single-page-application"
run_worker_first = ["/*", "!/", "!/console", "!/console/*"]
```

Workers 支持 D1 或 libSQL，不使用本地 SQLite、TCP 数据库或本地文件目录。需要 S3/R2 时，构建时启用 `s3` feature，并设置 `GPROXY_S3_ACCESS_KEY_ID`、`GPROXY_S3_SECRET_ACCESS_KEY` 和相应的文件存储配置。完整字段见[配置参考](/zh-cn/reference/configuration/)。

### 从源码构建

从仓库根目录执行：

```sh
rustup target add wasm32-unknown-unknown
cargo install worker-build
pnpm --dir console install --frozen-lockfile
pnpm --dir deploy/cloudflare install
pnpm --dir deploy/cloudflare build
pnpm --dir deploy/cloudflare check
```

构建与打包选项见[从源码构建](/zh-cn/deployment/release-build/)。

[cloudflare-auto]: https://deploy.workers.cloudflare.com/?url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fcloudflare-button
[cloudflare-external]: https://deploy.workers.cloudflare.com/?url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fcloudflare-external
[netlify-auto]: https://app.netlify.com/start/deploy?repository=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&branch=dev&create_from_path=deploy%2Fserverless
[netlify-external]: https://app.netlify.com/start/deploy?repository=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&branch=dev&create_from_path=deploy%2Fserverless#GPROXY_DATABASE_URL=
[vercel-auto]: https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fserverless&project-name=gproxy&repository-name=gproxy&env=GPROXY_ADMIN_PASSWORD%2CGPROXY_MASTER_KEY&products=%5B%7B%22type%22%3A%22integration%22%2C%22group%22%3A%22postgres%22%2C%22protocol%22%3A%22storage%22%7D%5D
[vercel-external]: https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fserverless&project-name=gproxy&repository-name=gproxy&env=GPROXY_ADMIN_PASSWORD%2CGPROXY_MASTER_KEY%2CGPROXY_DATABASE_URL
[deno]: https://console.deno.com/new?clone=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&path=deploy%2Fserverless
