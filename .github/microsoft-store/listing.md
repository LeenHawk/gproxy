# Store listing draft

Display name: GPROXY Gateway
Publisher display name: Leen Hawk
Suggested category: Developer tools
Website: https://gproxy.leenhawk.com/
Support: https://github.com/LeenHawk/gproxy/issues
Privacy: https://gproxy.leenhawk.com/deployment/code-signing/#privacy
License: AGPL-3.0-or-later application; see the repository LICENSE

## English

Short description:

A local gateway for your AI providers, with one API endpoint and a web console.

Description:

GPROXY runs an LLM gateway on your Windows computer. Connect the AI providers
you already use, manage credentials and routes in a browser, and give compatible
clients a shared API endpoint.

- Connect supported AI providers using your own accounts or API credentials.
- Route requests between providers and configure failover.
- Manage access, quotas, usage accounting and pricing rules.
- Use OpenAI, Claude and Gemini-compatible request formats where supported.
- Manage the local gateway through its built-in web console.

The app starts a background local server. Open GPROXY from the Start menu to
complete setup and open the console. Provider accounts, availability, fees and
terms are separate from this app. Requests are sent to the providers you
configure; usage and logs are stored according to your settings.

## 简体中文

简短说明：

在 Windows 本机运行的 AI 网关，以统一 API 和网页控制台管理你的模型供应商。

说明：

GPROXY 在你的 Windows 电脑上运行 LLM 网关。接入已有的 AI 供应商，在浏览器中
管理凭据和路由，让兼容客户端通过统一 API 地址访问模型。

- 使用自己的账号或 API 凭据接入支持的 AI 供应商。
- 配置请求路由和故障切换。
- 管理访问权限、额度、用量统计和计价规则。
- 在支持的场景中使用 OpenAI、Claude 和 Gemini 兼容请求格式。
- 通过内置网页控制台管理本机网关。

应用会启动本机后台服务。从开始菜单打开 GPROXY，完成首次设置并进入控制台。
供应商账号、可用性、费用和条款独立于本应用。请求发送到你配置的供应商，
用量和日志按照你的设置保存。

## Certification notes for runFullTrust

GPROXY is a native Rust developer tool that listens on a user-configured HTTP
port (loopback by default), forwards authorized requests to user-configured
AI services and provides a browser-based management console. The full-trust
launcher invokes an included PowerShell first-run setup helper and starts the
native gateway. No elevation or system service is required. App data is stored
in the package LocalState directory. Login startup is controlled by Windows
Startup settings. Store-installed copies do not replace their own executable;
all application updates are distributed through Microsoft Store.

Supply fresh Windows screenshots and complete the actual certification
questionnaire; this draft does not assert that Store certification has passed.
