---
title: 控制台、门户与公开站点
description: "gproxy 二进制提供的三个 Web 界面、控制台每个板块管理的内容，以及控制台如何构建并嵌入二进制"
---

`gproxy` 二进制在三个路径上提供同一个 React 应用。构建产物嵌入在二进制里，无
需额外部署任何东西。

| 路径 | 界面 | API | 面向 |
| --- | --- | --- | --- |
| `/` | 公开站点 | 无 | 任何能访问该端口的人 |
| `/admin` 与 `/admin/*` | 操作员控制台 | `/admin/api/**` | 管理员 |
| `/portal` | 用户门户 | `/portal/api/**` | 设有密码的用户 |

端口上的其余流量都是以 API 密钥认证的网关流量（见
[路由与端点](/zh-cn/reference/routing-table/)）。

## 首次启动

在管理员存在之前，`GET /admin/api/session` 返回 `setup_required: true`。此时
控制台显示 **创建管理员**；`POST /admin/api/setup` 接受一个用户名和密码，创建
第一个管理员，打开会话并记录一条 `auth.setup` 审计事件。该表单按来源地址限流
为每分钟四次尝试。

如需跳过表单，启动二进制时设置 `GPROXY_ADMIN_PASSWORD`（可选
`GPROXY_ADMIN_USER`，默认 `admin`）。账户在首次运行时创建，管理员 API 密钥自
动生成或取自 `GPROXY_BOOTSTRAP_ADMIN_API_KEY`，`GPROXY_BOOTSTRAP_CHANNELS`
可为列出的通道 id 创建空 Provider。引导密钥与通道只在首次运行时生效，但指定
管理员的密码会在每次启动时重新应用，因此登录后请移除
`GPROXY_ADMIN_PASSWORD`。见[配置](/zh-cn/reference/configuration/)。

## 登录

控制台通过 `POST /admin/api/login` 登录并持有 `gproxy_admin_session`
Cookie：HttpOnly、`SameSite=Strict`、限定 `/admin` 路径、有效期 12 小时。使用
Cookie 发起的写操作必须同源。脚本可用 `Authorization: Bearer <api-key>` 调用
管理 API，密钥须属于标记为 `is_admin` 的用户；Bearer 调用不做同源检查。登录
与登出分别审计为 `auth.login` 和 `auth.logout`。

## 控制台板块

侧边栏列出十个板块。路径都是可收藏的真实 URL。

| 板块 | 路径 | 管理内容 |
| --- | --- | --- |
| 概览 | `/admin` | 健康凭证比例、需要关注的凭证、24 小时内的请求数与结算成本、7 天的每小时用量趋势、按 Provider 的支出、达到或超过 80% 的配额窗口与上游周期。 |
| 供应商 | `/admin/providers` | Provider 列表与详情标签页：凭证（凭证池、登录向导、健康、配额周期）、模型（服务的模型、变体、单模型定价）、规则、路由规则、设置（通道字段、端点覆盖、代理、TLS 指纹、转发元数据）。 |
| 负载均衡 | `/admin/routes` | 路由：名称、最大尝试次数、成员（Provider、固定凭证、上游模型、故障转移层级、权重）、带公开元数据的模型映射、路由别名与模型别名。 |
| 规则 | `/admin/rules` | 规则集、其变更规则与实际顺序，以及 Provider 附加关系。 |
| 身份 | `/admin/identity` | 组织、团队、用户与 API 密钥；各作用域上的权限、限流与配额，并显示继承值。 |
| 统计 | `/admin/usage` | 用量、管理操作（`/admin/audit`）与请求审计（`/admin/logs`）三个标签页。 |
| 定价 | `/admin/pricing` | 按模型模式的价格规则、维度费率、上下文与服务层级阶梯。 |
| 分词器词表 | `/admin/tokenizers` | 词表开关、自动获取、默认词表、Hugging Face token、已缓存词表（带进度的获取、删除）。 |
| 更新 | `/admin/update` | 更新通道与自动检查偏好、签名更新检查、应用、回滚、发布说明。仅原生构建。 |
| 设置 | `/admin/settings` | 实例设置、全局元数据黑名单、保留与捕获、配置导出与导入、门户设置、登录时启动。 |

打开自动检查且所选通道上存在更新构建时，每个页面上方都会出现更新横幅。原生
二进制还会显示签名公告源。侧边栏底部打印构建标识：版本、通道、短哈希和安装
类型。

## 实例设置

`GET` 与 `PATCH /admin/api/instance-settings` 携带以下键；设置、分词器词表和
更新页面编辑的是同一条记录的不同子集。

| 键 | 含义 |
| --- | --- |
| `instance_name` | 写入每条用量行维度的标签。默认 `default`。 |
| `proxy` | 默认上游代理 URL，在凭证与 Provider 的覆盖之后使用。 |
| `inherit_system_proxy` | 没有显式代理时遵循 `HTTP_PROXY` 与 `HTTPS_PROXY`。默认关闭。 |
| `enable_usage` | 结算后持久化用量行。默认开启；关闭后准入与计费仍然执行。 |
| `enable_tokenizer_vocabs`、`enable_tokenizer_download`、`default_tokenizer_vocab` | 用真实词表计数 Token、自动获取缺失词表，以及回退词表。 |
| `file_upload_max_in_flight` | 并发文件上传数；`0` 为不限。`GPROXY_FILE_UPLOAD_MAX_IN_FLIGHT` 优先。 |
| `retention_days`、`max_database_size_mb` | 可观测性清理边界；至少设置一个才能开启请求体捕获。 |
| `enable_downstream_log`、`enable_downstream_log_body`、`enable_upstream_log`、`enable_upstream_log_body`、`disable_log_redaction` | 线上捕获与脱敏，见[用量、日志与审计](/zh-cn/guides/observability/)。 |
| `update_channel`、`enable_auto_update_check` | `releases`、`staging` 或 `dev`；未设置时跟随二进制构建时的通道。 |
| `traffic_blacklist` | 在任何通道允许列表之前，实例范围内额外剔除的请求头、响应头与查询参数。 |

门户唯一的设置——用户能否看到最近结算请求——位于 `GET` 与
`PATCH /admin/api/portal-settings`。设置页的 **测试连通性** 通过已保存的代理
链探测出口，并报告上游会看到的地址。

## 主题与语言

每个界面都提供 English、简体中文和繁體中文。控制台与门户还提供浅色、深色和
跟随系统三种主题；选择保存在浏览器的 `gproxy-console-theme` 中。公开站点只显
示语言菜单。

## 深链接

| URL | 打开 |
| --- | --- |
| `/admin/providers/<id>/<tab>` | 某个 Provider 的 `credentials`、`models`、`rules`、`routing` 或 `settings` 标签页；`/credentials/<credentialId>` 打开单个凭证。 |
| `/admin/routes/<id>/models`、`/admin/routes/<id>/settings`、`/admin/routes/new` | 某个路由的标签页，或创建表单。 |
| `/admin/identity/<users\|teams\|organizations>/<id>` | 某个身份实体。`/admin/keys/...` 是其别名。 |
| `/admin/logs/<request_id>` | 一条捕获的请求及其上游尝试。 |
| `/portal?oauth_return=/<path>` | 门户登录后继续跳转到同源路径；CLI 登录流程会用到。 |

## 键盘与小屏幕

可打开详情的表格行和卡片可获得焦点，响应 Enter 和空格。侧边栏与工作区的拖动
手柄接受方向键、Home 和 End，并记住宽度。低于 `lg` 断点时侧边栏变为可横向滚
动的条；低于 `md` 时列表加详情的工作区一次只显示一栏并提供返回按钮，数据表
渲染为卡片。

## 用户门户

任何设有密码的用户都可以在 `/portal` 登录（`POST /portal/api/login`；Cookie
`gproxy_portal_session`，12 小时）。管理员在身份板块创建用户并设置初始密码；
用户在门户中修改密码。

| 面板 | 作用 |
| --- | --- |
| 账户 | 修改密码。创建前缀为 `sk`（API 客户端）或 `at`（Codex access-token 登录）的 API 密钥并可加标签；密钥只显示一次。列出并撤销自己的密钥。 |
| 连接 | 选择一个获准使用的模型，复制可直接运行的片段：curl、OpenAI Python、Claude Python、Gemini Python、Codex CLI 配置、Claude Code 环境变量。片段仅限该模型能服务的线上格式。 |
| 获准使用的模型 | 账户可调用的实时路由及其能力。 |
| 用量与成本 | 1、7 或 30 天内已结算的请求数、输入、输出与缓存 Token 以及成本。 |
| 配额窗口 | 作用于用户、团队和组织的支出窗口：总量、每日、每周、每月、5 小时与 7 天。 |
| 最近结算请求 | 最近 20 条请求，含 Provider、操作、上游模型、Token、成本与延迟。仅在管理员启用后显示；永不显示请求体。 |

密钥形如 `<prefix>-gp-<random>`。Codex 与 Claude Code 片段在
[CLI 客户端](/zh-cn/guides/cli-clients/)中说明。

## 公开站点

`/` 是一个落地页：一个可在 OpenAI Chat、Claude Messages 与 Gemini 之间切换的
请求转换示例、执行漏斗、产品要点、带模型占位符的连接示例，以及指向管理控制
台、门户、源码仓库和许可证的链接。

## 构建并嵌入控制台

控制台位于 `console/`，用 pnpm 管理。

```bash
cd console
pnpm install
pnpm build      # tsc -b, vite build, then scripts/sync-to-embed.mjs
```

最后一步把 `console/dist/` 复制到 `crates/gproxy-host-axum/assets/web/`，由
`rust-embed` 编译进二进制。之后重新构建 `gproxy`。没有该目录也能构建二进制并
提供 API；访问 `/` 时返回
`web assets are not embedded; run pnpm build in console/ and rebuild gproxy`。

开发时 `pnpm dev` 启动 Vite，并把管理与门户 API 代理到运行中的后端。控制台
改动以 `pnpm lint` 和 `pnpm test` 收尾。`console/src/generated/` 下的类型由
`ts-rs` 在 `cargo test` 期间从 Rust DTO 生成，绝不手工编辑。

嵌入的 `index.html` 服务于 `/`、`/admin`、`/admin/*` 和 `/portal`；`/assets/`
下带哈希的文件缓存一年，HTML 为 `no-cache`，`/build-info.js` 注入构建标识。
