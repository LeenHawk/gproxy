---
title: "CLI 客户端"
description: "把 Codex CLI 和 Claude Code 指向 GPROXY；模拟的厂商控制面哪些由本地回答、哪些转发，以及各自的限制"
---

一些厂商 CLI 不只访问推理端点：它们还会从厂商控制面获取账号资料、用量、
插件或文件。渠道可以声明一张 **服务面（service surface）** 表，列出这些
控制面路径，并为每条指定 GPROXY 是本地回答、在一个固定凭证上转发，还是把它
视为普通操作的别名。每一条都经过与普通请求相同的认证、权限、配额和结算路径，
因此 CLI 与其他客户端一样被计量。

## 命名前缀

控制面路径与厂商强相关，因此通过命名前缀访问：路径的第一段是 Provider、
路由或模型命名空间的名称，其余部分是厂商的原生路径。

```text
https://gproxy.example/codex/backend-api/codex/responses
                       ^^^^^ provider named "codex"
```

门户的 **连接** 卡片用你的地址、你的密钥和你选择的模型渲染下面的片段。当
模型带命名空间（`team-a/reviewer`）时，Base URL 变为
`https://gproxy.example/team-a`。

## Codex CLI

Codex 片段假定存在一个名为 `codex` 的 Provider（例如由
`GPROXY_BOOTSTRAP_CHANNELS=codex` 创建），或一个名为 `codex` 的模型命名空间。

```toml
# ~/.codex/config.toml
model = "gpt-5.4"
model_provider = "openai"
openai_base_url = "https://gproxy.example/codex/backend-api/codex"
chatgpt_base_url = "https://gproxy.example/codex/backend-api"
```

```sh
export CODEX_REFRESH_TOKEN_URL_OVERRIDE='https://gproxy.example/codex/oauth/token'
export CODEX_REVOKE_TOKEN_URL_OVERRIDE='https://gproxy.example/codex/oauth/revoke'
codex login --device-auth --experimental_issuer 'https://gproxy.example/codex' \
  --experimental_client-id app_EMoamEEZ73f0CkXaXp7hrann
```

### 登录

GPROXY 是 Codex 的 OAuth 签发端。`codex login --device-auth` 打印一个代码并
打开 `<issuer>/codex/device`；你登录门户、输入代码并批准。随后 Codex 在
`/codex/oauth/token` 完成交换。GPROXY 签发自己的 access token（有效一小时）
和 refresh token（30 天），为该门户用户创建一把标签为 `Codex OAuth` 的密钥，
并把每个携带该 token 的请求映射到这把密钥。刷新和撤销走上面的覆盖 URL。
浏览器流程（`/codex/oauth/authorize`，回调到 `localhost:1455` 或 `1457`）
的工作方式相同。

### 服务内容

| 路径 | 处理方式 |
| --- | --- |
| `/backend-api/codex`、`/backend-api/wham`、`/backend-api`、`/api/codex`、`/codex` 之下的 `responses`、`responses/compact`、`images/generations`、`images/edits`、`alpha/search`、`realtime/calls`、`memories/trace_summarize`、`guardian`、`guardian-classifier` | 普通操作的别名：路由、转换、故障转移、结算。 |
| `usage`、`usage/thread_usage/query`、`accounts/check`、`profiles/me`、`settings/user`、`workspace-messages`、`config/bundle`、`rate-limit-reset-credits`、analytics、`whoami`、workspace settings | 本地回答。用量报告调用方在 GPROXY 的已结算用量，以及该凭证观测到的 5 小时和 7 天窗口。 |
| `models`、`agent-identities`、`mcp`、plugins、connectors directory、`environments`、`tasks`、`files` | 在一个凭证上转发。MCP 会话按 `mcp-session-id` 固定；tasks 与 files 记住创建它们的凭证。 |
| `remote/control/...` | 在固定凭证上到厂商的 WebSocket 桥接。 |

Provider 设置 JSON 可以影响本地回答：`codex_pat_plan_type`（`free`、`go`、
`plus`、`pro`、`team`、`business`、`enterprise`、`edu`；默认 `pro`）、
`codex_virtual_settings`、`codex_workspace_messages`、`codex_config_bundle`、
`codex_plugins_enabled`。

### 限制

非流式 Responses 请求在上游转换为流式。Token 统计由本地回答。`codex` 渠道不
支持 embeddings。线程级用量为空；通过 CLI 查询的限流重置卡始终为无，控制台
可以使用它们。

## Claude Code

```sh
export ANTHROPIC_BASE_URL='https://gproxy.example'
export CLAUDE_CODE_OAUTH_TOKEN='sk-gp-...'
claude --model 'claude-sonnet-4-6'
```

Claude Code 以 `Authorization: Bearer` 发送该 token，准入把它当作 GPROXY 密钥
读取。Base URL 就是原始地址：Messages、统计 Token 和模型列表像任何聚合请求一
样按模型名解析，控制面路径则对照 `claudecode` 渠道上的每个 Provider 匹配。

| 路径 | 处理方式 |
| --- | --- |
| `/api/hello`、`/api/claude_cli/bootstrap`、`/api/claude_cli_profile`、`/api/claude_code_penguin_mode`、`/api/claude_code/skills`、`/api/oauth/organizations/{org}/skills/...` | 本地回答，账号与组织 id 由 Provider 和用户推导生成。 |
| `/api/oauth/file_upload`、`/v1/files`、`/v1/files/{id}`、`/v1/files/{id}/content` | 在一个凭证上的 Files API；上传会把文件绑定到存储它的凭证。 |
| `/v1/skills`、`/v1/skills/{id}`、`/v1/skills/{id}/versions/...` | Skills API，绑定方式相同。 |

可选的 Provider 设置：`claudecode_bootstrap`、`claudecode_fast_mode`、
`claudecode_skill_health`、`claudecode_shared_skills`（JSON 原样返回）。

技能归档下载返回 `404`。Claude Code 显示的账号、组织和套餐信息是 GPROXY 的
合成值，而不是上游账号的真实值。

## 会话亲和

多轮 CLI 在一个会话始终落在同一凭证时表现最好。GPROXY 从任何请求的
`x-gproxy-session-id` 推导会话主体；OpenAI 形态的请求还可用 `session-id`、
`x-session-id` 或 `thread-id`；Claude Messages 可用 `x-claude-code-session-id`
或 `session_id`。固定关系在一小时无活动后失效。配合 `sticky` 凭证策略，
同一主体也会选中同一凭证。

## Gemini CLI

`geminicli` 是一个上游渠道：它汇聚 Gemini CLI 的 OAuth 凭证，并用它们服务
Gemini、OpenAI 和 Claude 客户端。GPROXY 不模拟 Gemini CLI 的控制面，门户也
没有 Gemini CLI 片段。Google GenAI SDK 把 Base URL 设为你的地址、把密钥放在
`x-goog-api-key` 即可连接，见[发送第一个请求](/zh-cn/getting-started/first-request/)。
