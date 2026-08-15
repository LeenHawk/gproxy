# 📌 提交 Issue 前请阅读 / Please Read Before Opening an Issue

感谢你帮助改进 GPROXY。信息完整、可复现的 Issue 能让问题更快得到定位和处理。

Thank you for helping improve GPROXY. Complete, reproducible reports make it much
easier to diagnose and resolve problems quickly.

## 中文

### 提交前

1. 搜索[现有 Issues](https://github.com/LeenHawk/gproxy/issues)，确认问题尚未被报告。如果已有相同
   Issue，请补充信息或用 reaction 表示你也遇到了，不要重复提交。
2. 查阅[文档](https://gproxy.leenhawk.com/zh-cn/)和
   [最新 Release](https://github.com/LeenHawk/gproxy/releases/latest)。如果可以，请先用最新版本验证。
3. 每个 Issue 只描述一个问题或一项功能请求，并使用具体、可搜索的标题。

### Bug 报告需要包含

- **GPROXY 版本**：完整版本号或 commit SHA；
- **运行环境**：部署方式（Docker、原生二进制、Cloudflare 等）、操作系统及架构；
- **请求路径**：使用的客户端/协议、Provider、模型，以及聚合或 scoped 路由；
- **复现步骤**：最小、完整、可重复执行的步骤或请求示例；
- **期望结果与实际结果**：说明你认为应当发生什么，以及实际发生了什么；
- **诊断信息**：相关日志、错误堆栈、HTTP 状态码、时间戳，以及
  `x-gproxy-request-id`（如有）；
- **相关配置**：只提供复现问题所需的最小配置，并对所有敏感值进行脱敏。

请优先粘贴可搜索的文本，不要只提供日志截图。如果问题只在特定上游账号下出现，
请说明这一点，但不要提供账号或凭据本身。

### 功能请求需要包含

- 你想解决的实际问题和使用场景；
- 希望的行为，最好附上简短示例；
- 你已经尝试过的替代方案；
- 对现有用户、API 或配置兼容性可能产生的影响。

### 安全与隐私

**绝对不要提交 API key、access token、cookie、密码、完整凭据、私有请求内容或其他敏感信息。**
请用 `<redacted>` 等明确占位符替换它们，并在发布前再次检查日志和截图。
对于尚未公开且可能被利用的安全漏洞，请不要创建公开 Issue；请使用本仓库
Security 页面提供的私密报告方式（如可用）。

### 可直接使用的模板

```markdown
## 问题描述

## 复现步骤
1.
2.
3.

## 期望结果

## 实际结果

## 环境
- GPROXY 版本/commit：
- 部署方式：
- 操作系统/架构：
- 客户端与协议：
- Provider 与模型：
- 路由方式（聚合/scoped）：

## 日志与请求 ID

## 补充信息
```

维护者可能会请你补充信息，合并或关闭重复 Issue，或关闭因缺少必要信息而无法调查的
报告。感谢你花时间让报告变得可操作。

---

## English

### Before opening an issue

1. Search the [existing issues](https://github.com/LeenHawk/gproxy/issues) to make
   sure the problem has not already been reported. If it has, add useful context
   or react to the existing issue instead of opening a duplicate.
2. Check the [documentation](https://gproxy.leenhawk.com/) and the
   [latest release](https://github.com/LeenHawk/gproxy/releases/latest). When
   possible, verify the problem with the latest version first.
3. Keep each issue focused on one problem or feature request, and use a specific,
   searchable title.

### Bug reports should include

- **GPROXY version:** the full version number or commit SHA;
- **Environment:** deployment method (Docker, native binary, Cloudflare, etc.),
  operating system, and architecture;
- **Request path:** client/protocol, provider, model, and whether aggregated or
  scoped routing is used;
- **Reproduction steps:** a minimal, complete, repeatable sequence or request;
- **Expected and actual behavior:** what you expected and what happened instead;
- **Diagnostics:** relevant logs, stack traces, HTTP status, timestamp, and the
  `x-gproxy-request-id`, when available;
- **Relevant configuration:** only the minimum configuration needed to reproduce
  the problem, with every sensitive value redacted.

Prefer searchable text over screenshots of logs. If the problem only occurs with
a particular upstream account, say so without sharing the account or its
credentials.

### Feature requests should include

- The concrete problem and use case you want to address;
- The behavior you would like, ideally with a short example;
- Alternatives or workarounds you have already tried;
- Possible compatibility impact on existing users, APIs, or configuration.

### Security and privacy

**Never post API keys, access tokens, cookies, passwords, complete credentials,
private request content, or other sensitive information.** Replace them with an
explicit placeholder such as `<redacted>`, and review logs and screenshots again
before posting. Do not open a public issue for an undisclosed, potentially
exploitable vulnerability; use the private reporting method shown on the
repository's Security page, when available.

### Copyable template

```markdown
## Description

## Steps to reproduce
1.
2.
3.

## Expected behavior

## Actual behavior

## Environment
- GPROXY version/commit:
- Deployment method:
- OS/architecture:
- Client and protocol:
- Provider and model:
- Routing mode (aggregated/scoped):

## Logs and request ID

## Additional context
```

Maintainers may ask for more information, close duplicates, or close reports that
cannot be investigated because essential details are missing. Thank you for
taking the time to make your report actionable.
