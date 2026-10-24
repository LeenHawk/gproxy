---
title: OrcaTerm
description: "通过 OrcaTerm 桌面端的 LightAI 协议接入账号，支持浏览器 OAuth、令牌刷新、账号额度、动态模型目录和文本对话。"
---

`orcaterm` 渠道使用 OrcaTerm 1.6.6 内置的 LightAI 协议，支持系统提示、多轮历史、流式回答和思考内容。GPROXY 对外提供 OpenAI Chat Completions，并通过现有转换支持 Responses、Claude Messages 和 Gemini。

## 接入步骤

1. 新建 **OrcaTerm** 供应商。国内服务的基础地址留空即可；国际站账号启用 `international`。
2. 选择默认的**设备授权**登录方式，打开授权链接并完成浏览器登录。GPROXY 自动轮询并保存 OAuth 凭证，无需复制设备码、Cookie 或回调 URL。国内默认微信，国际站默认 Google；通过 `oauth_provider` 可选择 `wechat`、`txcloud`（腾讯云）、`google` 或 `github`，具体可用方式由 OrcaTerm 所选区域决定。
3. 刷新供应商的模型目录。模型来自桌面端使用的动态 Rainbow 配置，请保留返回的完整模型 ID，包括 `TokenHub/`、`Hunyuan3/` 等前缀。
4. 为导入的模型分配访问权限，即可通过 GPROXY 发送请求。

已有凭证通过 `POST /admin/api/credentials`（或批量接口）导入，再通过 `POST /admin/api/credentials/{id}/refresh` 初始化。控制台保存后自动刷新待初始化的记录；失败后保留已保存记录，可在详情中重试。Claude Web 使用 `authKind: "cookie"`；Claude Code 和 OrcaTerm 使用 `authKind: "oauth"`，也可提交 `secret: {"cookie": "..."}`。历史类型标签继续兼容读取。初始化和刷新将 secret、到期时间及可选公开 metadata 一起按版本 CAS 保存，无需数据库迁移或历史回填。 `secret: {"cookie": "ot_session=...; skey=..."}` / `secret: {"access_token": "..."}`.

令牌刷新与客户端一致：调用 `OAuthRefreshToken` 并传入当前访问令牌，不虚构独立的 refresh-token 授权流程。

`base_url` 覆盖 LightAI 服务地址，不含 `/assistant`；`site_url` 覆盖登录与刷新的账号服务地址。默认值如下：

| 区域 | LightAI | 账号服务 |
| --- | --- | --- |
| 国内 | `https://lightai.cloud.tencent.com` | `https://orcaterm.com` |
| 国际站 | `https://lightai.tencentcloud.com` | `https://orcaterm.ai` |

## 账号额度

凭证的额度查询调用 `POST /cgi/api?action=DescribeUserUsageStats`，RPC 的 `data` 为 `{"feature":"hunyuan_ai"}`。它对应桌面端的 **AI 问答次数额度**，展示已用量、上限、剩余量和使用比例。上限 `-1` 表示不限额；缺失的计数保持未知。空响应、格式错误或其他功能的额度不会被当作零余额。

2026-10-24 检查的公开 `DescribeProductEditions` 接口标示：基础版 1000 次/月，专业版 6000 次/月，试用和域名类套餐存在按日额度。这些是公开套餐定义，不是某个账号的实测余额，也不会硬编码进额度查询。请求次数不会被转换成资金余额或 token 余额。

仅当 AI 用量自身返回带明确时区的 `quotaStartDate` / `quotaEndDate` 时，才展示对应周期边界。不会根据无时区日期、订阅到期时间或固定月度规则推算重置时间，也不会自动购买额度或执行额度重置。

该计数器未说明当前哪些 LightAI 模型共用额度，因此这里只作**观测和展示**。读数会进入正常额度视图，但不会据此在本地封禁所有模型，也不会将网关请求累加到未经确认的额度池。上游自身的额度耗尽响应仍照常处理。

匿名用量查询返回 `VERIFY_USER_FAILED`；个人实际剩余量需要登录账号，目前尚未完成真实账号验证。

## 支持范围

当前仅支持**文本**。图片、音频、文件内容、客户端函数工具、工具历史、强制工具选择、结构化输出和多候选回答都会被拒绝。上游要求调用 OrcaTerm 工具时会返回错误；GPROXY 不会替调用者执行终端命令或批准工具操作。

每个请求使用新的上游会话 ID，并显式传递调用者提供的历史，避免不同下游对话共用上游会话。这不是零留存模式，服务端历史保留策略由 OrcaTerm 控制。渠道不导入连接配置、SSH 凭据、终端录制或桌面遥测。

已核对的 SSE 协议没有可验证的 token 计数字段，因此用量保持“未报告”，不会伪装成零。账号额度单独查询，不声明付费续用、额度重置或 WebSocket 能力。模型发现覆盖内置模型，不会把账号私有的第三方模型配置冒充成内置模型 ID。下游模型列表仍来自 GPROXY 本地、经过权限过滤的目录。

2026-10-24 已验证公开模型目录和匿名 OAuth 入口：国内微信、腾讯云以及国际站 Google 均跳转到相应授权页面。未完成授权的 `OAuthExchangeToken` 返回 `TOKEN_EXPIRED`，渠道在本地十分钟登录窗口内将它视为等待授权，其他错误不会被隐藏成等待状态。

成功授权、生成和刷新仍需真实账号，尚未完成带认证的端到端实测。脚本化传输检查仅验证请求和响应处理，不证明账号权益或真实上游调用成功。国际站地址选择依据桌面端分支，并非另外捕获的国际版安装实例。

## 协议依据

官方 Windows x64 标准版安装包：

```text
https://orcaterm-desktop-1258344699.cos.ap-guangzhou.myqcloud.com/release/latest/OrcaTerm-latest-win-x64.exe
SHA-256 f42940c099bc32fbe668adfea25fd5353f2b711b1e705536638aa1f78b28c173
```

已提取连续 PE 资源表中的全部 450 项并完成 Brotli 解压。本地恢复的厂商代码保存在 `samples/orcaterm-1.6.6/`，该目录不纳入版本控制。关键 webpack 模块及格式化代码位置如下：

| 来源 | 协议事实 |
| --- | --- |
| 模块 `2481`，`callSubAgent` 与 `createCallHandler` | `/assistant/chat`、`input.historyMessages`、UUID、`name`、`mode`、`csrfCode` 和 SSE 请求头 |
| 模块 `26196` | `data:` 流格式、内容和思考增量、`[DONE]`、`[ERROR]:...`、`[ABORTED]` |
| 模块 `82025`、`92054` | 输入动作枚举和 `user.setting` 封装 |
| `features.js:40277` | Agent 的 `completion.taskCompletion`、`ask.question`、`review.tool` 及思考内容映射 |
| `features.js:6660` | 各区域 LightAI 地址 |
| `features.js:14836` | `modelDesc.kwargs` 中的模型思考参数 |
| `features.js:818` | 公开 Rainbow 请求及 `orcaterm.内置模型` 分组 |
| `features.js:675` | 正权重模型记录和按账号选择加权变体 |
| `app.js:6210`、`features.js:87360` | 浏览器授权 URL、会话 UUID 和桌面回调交接 |
| `oauth-browser-redirects.json`、`oauth-pending-session.json` | 匿名实测的授权跳转和待授权响应 |
| `ot_setting.js` 的 `Kt` / `Zt`；`features.js:94967`、`77060` | AI 用量请求、已用/上限、不限额标记和次数显示 |
| `DescribeProductEditions-anonymous.json` | 公开套餐周期（`update_span`、`update_unit`）和标示上限 |
| `DescribeUserUsageStats-anonymous.json` | 用量接口要求账号认证 |
| `features.js:87182` | `GetUserInfo` 身份字段 |
| `features.js:88116`、`88231`、`88310` | `ot_session`、OAuth 交换/刷新及有效期 |
| `features.js:88370` | 明确的会话/刷新令牌拒绝码与临时失败重试 |
| `app.js:2920`、`2057` | 账号 RPC 封装与令牌刷新动作 |
| `http-interceptor.js` | 桌面身份和 Cookie 注入；UA 字面量中的产品版本仍是 `1.0.0` |

无需凭证的公开模型目录请求：

```http
POST https://dnspod.cloud.tencent.com/ajax/rainbow?action=getRainbowConfig
Content-Type: application/json

{"app":"ai","groups":["orcaterm.内置模型"],"options":{"enableEnv":1}}
```

响应中的 `model` 是实际发送给上游的模型选择器，`displayName` 和 `text` 是显示名称。停用的记录（`weight=0`）会被过滤；发现失败时不会用硬编码快照替代。
