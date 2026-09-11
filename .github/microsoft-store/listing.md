# Store listing draft

Display name: GPROXY Gateway
Publisher display name: Leen Hawk
Suggested category: Developer tools
Website: https://gproxy.leenhawk.com/
Support: https://github.com/LeenHawk/gproxy/issues
Privacy: https://gproxy.leenhawk.com/deployment/code-signing/#privacy
License: AGPL-3.0-or-later application; see the repository LICENSE

## English

GPROXY Gateway brings your AI providers together behind one local API endpoint. Run the gateway on your Windows computer and manage it through a built-in browser console.

• Connect supported AI providers using your own accounts or API credentials.
• Organize provider credentials and configure load balancing and failover.
• Use OpenAI, Claude and Gemini-compatible API formats where supported.
• Manage access permissions, quotas, usage records and pricing rules.
• View request trends, provider spending and credential status in the dashboard.

Open GPROXY Gateway from the Start menu to complete first-run setup and launch the console. The gateway continues running in the background. Microsoft Store manages updates, and Windows Startup settings control whether the app starts when you sign in.

Provider accounts, model availability, fees and terms are separate from this app. Requests are sent to the providers you configure. Data storage, logging and retention follow your settings.

## 简体中文

GPROXY Gateway 将多个 AI 供应商接入统一的本机 API 地址。在 Windows 电脑上运行网关，通过内置网页控制台管理模型访问与请求路由。

• 使用自己的账号或 API 凭据接入支持的 AI 供应商。
• 统一管理供应商凭据，配置负载均衡和故障切换。
• 在支持的场景中使用 OpenAI、Claude 和 Gemini 兼容 API 格式。
• 管理访问权限、额度、用量记录和计价规则。
• 在总览中查看请求趋势、供应商花费和凭据状态。

从开始菜单打开 GPROXY Gateway，完成首次设置并进入控制台。网关将在后台持续运行。更新由 Microsoft Store 管理，登录时自动启动可通过 Windows 启动设置控制。

供应商账号、模型可用性、费用和服务条款独立于本应用。请求会发送到你配置的供应商，数据存储、日志和保留期限按照你的设置执行。

## 繁體中文

GPROXY Gateway 將多個 AI 供應商整合至統一的本機 API 位址。在 Windows 電腦上執行閘道，透過內建網頁管理介面管理模型存取與請求路由。

• 使用自己的帳號或 API 憑證連接支援的 AI 供應商。
• 統一管理供應商憑證，設定負載平衡與容錯移轉。
• 在支援的情境中使用 OpenAI、Claude 和 Gemini 相容 API 格式。
• 管理存取權限、配額、用量紀錄與計價規則。
• 在總覽中查看請求趨勢、供應商花費與憑證狀態。

從開始功能表開啟 GPROXY Gateway，完成初次設定並進入管理介面。閘道將在背景持續執行。更新由 Microsoft Store 管理，登入時自動啟動可透過 Windows 啟動設定控制。

供應商帳號、模型可用性、費用與服務條款獨立於本應用程式。請求會傳送至你設定的供應商，資料儲存、日誌與保留期限依照你的設定執行。

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
