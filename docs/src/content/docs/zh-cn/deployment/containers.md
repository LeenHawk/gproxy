---
title: Northflank、Render 和 Heroku
description: 使用原生 GPROXY 容器与托管 PostgreSQL 部署网关。
---

这些模板将 **v4.0.3** PostgreSQL 预编译程序打包进轻量 Node.js 容器，运行一个应用实例并创建托管数据库。它们复用现有 HTTP/SSE/WebSocket 转发入口，并校验发布包的 SHA-256，无需编译 Rust。普通 CLI 容器只包含 SQLite 驱动，不能直接替代此镜像。应用和数据库可能产生费用，部署前请核对所选套餐。

## Render

[![Deploy to Render](https://render.com/images/deploy-to-render-button.svg)](https://render.com/deploy?repo=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev)

1. 点击按钮，检查仓库根目录 `render.yaml` Blueprint：Starter Web 服务和 Basic PostgreSQL 数据库。
2. 部署这两个资源。数据库连接自动绑定，管理员密码和加密主密钥由 Render 生成。
3. 在服务的 **Environment** 设置中查看并保存 `GPROXY_ADMIN_PASSWORD` 和 `GPROXY_MASTER_KEY`。打开服务地址的 `/console/`，使用 `admin` 登录。

数据库默认只接受内网连接。模板采用付费数据库，避免免费数据库到期失效。健康检查使用 `8787` 端口的 `/healthz`。

## Heroku

[![Deploy to Heroku](https://www.herokucdn.com/deploy/button.svg)](https://heroku.com/deploy?template=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev)

1. 用 `openssl rand -hex 32` 生成并保存主密钥。
2. 点击按钮，选择应用名和区域，将主密钥填入 `GPROXY_MASTER_KEY`。
3. 核对 Basic Web dyno 和 `heroku-postgresql:essential-0` 数据库插件，然后部署。
4. 在 **Settings → Reveal Config Vars** 中查看 `GPROXY_ADMIN_PASSWORD`，打开 `/console/` 并使用 `admin` 登录。

仓库根目录的 `app.json` 和 `heroku.yml` 使用 `deploy/serverless/Dockerfile`。Node.js 入口监听 Heroku 动态分配的 `PORT`，共享网关读取 `DATABASE_URL`，并支持 Heroku 分配的运行用户。本地文件是临时的；配置、凭证、用量和缓存状态保存在 PostgreSQL 中。

## Northflank

[打开 Northflank 模板编辑器](https://app.northflank.com/s/account/templates/new) · [模板 JSON](https://github.com/LeenHawk/gproxy/blob/dev/deploy/northflank/template.json)

1. 在 Northflank 账户或团队中创建模板，切换到 JSON 编辑器，粘贴 `deploy/northflank/template.json`。
2. 将模板命名为 GPROXY；按需调整模板参数 `PROJECT_NAME` 和 Project 节点的 `region`（默认 `europe-west`），保存并运行。
3. 工作流会创建项目、私有 PostgreSQL 数据库、密钥组和公开的 GPROXY 服务。应用与数据库均使用 `nf-compute-20`，数据库申请 4 GiB 存储；运行前请核对套餐。
4. 在项目的 `gproxy-secrets` 密钥组中查看并保存自动生成的 `GPROXY_ADMIN_PASSWORD` 和 `GPROXY_MASTER_KEY`。打开服务的公开 HTTPS 地址，在 `/console/` 使用 `admin` 登录。

Northflank 的[模板分享链接](https://northflank.com/docs/v1/application/infrastructure-as-code/share-a-template)需要在账户中保存模板后生成。仓库目前提供可导入模板，尚未发布 Northflank 分享链接。要提供直接部署按钮，请先保存模板并生成公开分享链接，分享前检查访问范围和有效期。

密钥组自动将数据库的 `POSTGRES_URI` 绑定为 `GPROXY_DATABASE_URL`。服务从仓库 `dev` 分支构建 `deploy/serverless/Dockerfile`，构建上下文为 `deploy/serverless`。重复运行模板时，Northflank 会保留已生成的密钥。就绪检查使用 `8787` 端口的 `/healthz`。

## 运行与更新

- 更新时保留数据库和 `GPROXY_MASTER_KEY`，并备份两者。这些模板读取 `GPROXY_DATABASE_URL` 或平台提供的 `DATABASE_URL`。
- 升级时修改 `deploy/serverless/Dockerfile` 中的 `GPROXY_RELEASE_VERSION`，再重新构建部署。所选发布需包含两种架构的 serverless Linux 压缩包和 `SHA256SUMS`。当前固定正式版本，不跟随 `nightly`。
- 模板默认运行一个应用实例，复用现有 serverless host，将共享缓存保存在 PostgreSQL 中。
- 使用 OAuth 或公开文件链接时，将 `GPROXY_PUBLIC_BASE_URL` 设为服务的公开 HTTPS 地址。这些模板不启用文件存储，需要文件系统或对象存储时请选择自定义原生 CLI 构建。客户端 IP 取自连接对端，可能是平台路由器地址；不信任转发头中的客户端 IP。
- 原生服务支持 HTTP、SSE 和 WebSocket，但仍受平台路由超时、空闲连接限制和实例重启影响。

Workers 和函数部署见 [Cloudflare、Netlify、Vercel 与 Deno 部署指南](/zh-cn/deployment/edge/)。
