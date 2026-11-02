# Security Policy / 安全策略 / Политика безопасности

[English](#english) · [简体中文](#简体中文) · [Русский](#русский)

---

## English

### Supported versions

| Version | Supported |
| --- | --- |
| Latest 4.x stable release | ✅ Security fixes |
| Older 4.x releases | ❌ Upgrade to the latest 4.x release |
| `beta` (`staging`) channel | ⚠️ Fixed as part of the next stable release |
| `dev` (`nightly`) channel | ⚠️ Best effort, no guarantees |
| 3.x and earlier | ❌ No longer supported |

Fixes are released as new 4.x patch versions. Please confirm the issue still
reproduces on the [latest stable release](https://github.com/LeenHawk/gproxy/releases/latest)
before reporting.

### Reporting a vulnerability

**Do not open a public issue, discussion or pull request for security problems.**

Report privately through one of:

1. GitHub private vulnerability reporting:
   <https://github.com/LeenHawk/gproxy/security/advisories/new> (preferred)
2. Email: <leenhawk@leenhawk.com>, with the subject starting with `[GPROXY SECURITY]`

Please include:

- Affected version, distribution (Desktop / Android / HarmonyOS / CLI /
  container / Cloudflare / Netlify / Vercel) and platform
- Relevant configuration (storage backend, whether `GPROXY_MASTER_KEY` is set,
  exposed listeners, reverse proxy in front)
- Steps to reproduce or a proof of concept, and the impact you observed
- Whether the issue is already public or known to others

**Redact all real secrets** — upstream API keys, OAuth tokens, cookies,
gateway keys, admin passwords and master keys — from logs, screenshots and
database dumps. Use throwaway credentials for testing.

### What to expect

GPROXY is maintained by a single developer, so timelines are best effort:

- Acknowledgement within 3 days
- Initial assessment within 7 days
- Fix target: about 30 days for critical/high severity issues, longer for lower severity

You will be kept informed during the process. After a fix is released, a
GitHub Security Advisory is published and reporters are credited unless they
prefer to remain anonymous. Please keep the details private until then
(coordinated disclosure, up to 90 days by default).

### Scope

In scope — vulnerabilities in code from this repository, for example:

- Authentication or authorization bypass for gateway keys, user accounts or the
  admin console / admin API
- Leakage of stored upstream credentials, OAuth tokens, cookies or gateway keys
  (through APIs, logs, error messages, exports or the console)
- Weaknesses in credential encryption or master key handling
- Cross-user data access, quota/billing bypass, or request log exposure between users
- SSRF, injection, path traversal or remote code execution
- XSS, CSRF or session issues in the web console
- Flaws in update checks, release signature/checksum verification or the
  official build and release pipeline

Out of scope:

- Vulnerabilities in upstream LLM providers or their APIs; report those to the provider
- Issues requiring an already compromised host, database, admin account or master key
- Documented behavior, for example credentials stored unencrypted when
  `GPROXY_MASTER_KEY` is not set, or request content recorded according to your
  logging settings
- Insecure deployments, such as exposing an instance to the public internet
  with weak or default admin credentials, or without TLS
- Denial of service through volume flooding, or missing rate limits without a
  concrete security impact
- Reports from automated scanners without a demonstrated, exploitable impact
- Provider terms-of-service or account ban questions
- Third-party forks, mirrors and unofficial builds

Thank you for helping keep GPROXY and its users safe.

---

## 简体中文

### 支持的版本

| 版本 | 支持情况 |
| --- | --- |
| 最新的 4.x 稳定版 | ✅ 提供安全修复 |
| 较旧的 4.x 版本 | ❌ 请升级到最新的 4.x 版本 |
| `beta`（`staging`）通道 | ⚠️ 随下一个稳定版一并修复 |
| `dev`（`nightly`）通道 | ⚠️ 尽力而为，不作保证 |
| 3.x 及更早版本 | ❌ 已停止支持 |

安全修复以新的 4.x 补丁版本发布。报告前请先确认问题在
[最新稳定版](https://github.com/LeenHawk/gproxy/releases/latest) 上仍可复现。

### 报告漏洞

**请勿通过公开的 Issue、Discussion 或 Pull Request 报告安全问题。**

请通过以下任一方式私下报告：

1. GitHub 私密漏洞报告：
   <https://github.com/LeenHawk/gproxy/security/advisories/new>（推荐）
2. 邮件：<leenhawk@leenhawk.com>，标题以 `[GPROXY SECURITY]` 开头

请尽量提供：

- 受影响的版本、发行形式（桌面端 / Android / HarmonyOS / CLI / 容器 /
  Cloudflare / Netlify / Vercel）及运行平台
- 相关配置（存储后端、是否设置 `GPROXY_MASTER_KEY`、对外监听情况、前置反向代理等）
- 复现步骤或概念验证（PoC），以及观察到的影响
- 该问题是否已公开或已被他人知晓

**请对所有真实密钥做脱敏处理**——包括上游 API Key、OAuth Token、Cookie、
网关密钥、管理员密码和主密钥——日志、截图和数据库导出中均不应出现。
测试时请使用临时凭据。

### 处理流程

GPROXY 由一名开发者维护，以下时间均为尽力而为：

- 3 天内确认收到报告
- 7 天内给出初步评估
- 严重 / 高危问题目标约 30 天内修复，较低等级问题可能需要更长时间

处理期间会与你保持沟通。修复发布后将公开 GitHub Security Advisory，
并在致谢中署名报告者（如希望匿名请告知）。在此之前请对细节保密
（协调披露，默认最长 90 天）。

### 范围

范围内——本仓库代码中的漏洞，例如：

- 网关密钥、用户账户或管理控制台 / 管理 API 的认证或授权绕过
- 已存储的上游凭据、OAuth Token、Cookie 或网关密钥泄露
  （通过 API、日志、错误信息、导出或控制台）
- 凭据加密或主密钥处理中的缺陷
- 跨用户数据访问、配额 / 计费绕过，或用户之间请求日志的泄露
- SSRF、注入、路径穿越或远程代码执行
- Web 控制台中的 XSS、CSRF 或会话问题
- 更新检查、发布签名 / 校验和验证，或官方构建与发布流水线中的缺陷

范围外：

- 上游大模型服务商或其 API 自身的漏洞，请向对应服务商报告
- 需要主机、数据库、管理员账户或主密钥已被攻破才能利用的问题
- 已在文档中说明的行为，例如未设置 `GPROXY_MASTER_KEY` 时凭据以明文存储，
  或按照你的日志设置记录请求内容
- 不安全的部署方式，例如以弱口令或默认管理员凭据、或在无 TLS 的情况下将实例暴露到公网
- 流量型拒绝服务，或没有具体安全影响的缺少限流问题
- 未证明可实际利用的自动化扫描器报告
- 服务商服务条款或账号封禁相关问题
- 第三方分支、镜像及非官方构建

感谢你帮助保障 GPROXY 及其用户的安全。

---

## Русский

### Поддерживаемые версии

| Версия | Поддержка |
| --- | --- |
| Последний стабильный выпуск 4.x | ✅ Исправления безопасности |
| Более ранние выпуски 4.x | ❌ Обновитесь до последнего выпуска 4.x |
| Канал `beta` (`staging`) | ⚠️ Исправляется в рамках следующего стабильного выпуска |
| Канал `dev` (`nightly`) | ⚠️ По возможности, без гарантий |
| 3.x и более ранние | ❌ Больше не поддерживаются |

Исправления выпускаются в виде новых патч-версий 4.x. Прежде чем сообщать о
проблеме, убедитесь, что она по-прежнему воспроизводится в
[последнем стабильном выпуске](https://github.com/LeenHawk/gproxy/releases/latest).

### Сообщение об уязвимости

**Не создавайте публичные issue, обсуждения или pull request по проблемам безопасности.**

Сообщайте конфиденциально одним из способов:

1. Закрытое сообщение об уязвимости на GitHub:
   <https://github.com/LeenHawk/gproxy/security/advisories/new> (предпочтительно)
2. Электронная почта: <leenhawk@leenhawk.com>, тема письма должна начинаться с `[GPROXY SECURITY]`

Пожалуйста, укажите:

- Затронутую версию, вариант поставки (Desktop / Android / HarmonyOS / CLI /
  контейнер / Cloudflare / Netlify / Vercel) и платформу
- Соответствующую конфигурацию (бэкенд хранилища, задан ли `GPROXY_MASTER_KEY`,
  открытые слушатели, обратный прокси перед шлюзом)
- Шаги воспроизведения или доказательство концепции (PoC), а также наблюдаемые последствия
- Является ли проблема уже публичной или известной другим

**Удалите все реальные секреты** — ключи API провайдеров, токены OAuth, файлы cookie,
ключи API шлюза, пароли администратора и мастер-ключи — из журналов, снимков экрана
и дампов базы данных. Для тестирования используйте одноразовые учётные данные.

### Чего ожидать

GPROXY сопровождает один разработчик, поэтому сроки соблюдаются по мере возможности:

- Подтверждение получения в течение 3 дней
- Первичная оценка в течение 7 дней
- Целевой срок исправления: около 30 дней для проблем критической и высокой серьёзности, дольше для менее серьёзных

Вас будут информировать на протяжении всего процесса. После выпуска исправления
публикуется GitHub Security Advisory, а авторы сообщений указываются в благодарностях,
если они не предпочитают оставаться анонимными. До этого момента, пожалуйста,
не раскрывайте подробности (скоординированное раскрытие, по умолчанию до 90 дней).

### Область действия

Входит в область действия — уязвимости в коде из этого репозитория, например:

- Обход аутентификации или авторизации для ключей API шлюза, учётных записей пользователей
  или консоли администратора / API администратора
- Утечка сохранённых учётных данных провайдеров, токенов OAuth, файлов cookie или ключей API шлюза
  (через API, журналы, сообщения об ошибках, экспорт или консоль)
- Недостатки в шифровании учётных данных или обработке мастер-ключа
- Доступ к данным других пользователей, обход квот или биллинга, раскрытие журналов запросов между пользователями
- SSRF, внедрение кода, обход каталогов (path traversal) или удалённое выполнение кода
- XSS, CSRF или проблемы с сеансами в веб-консоли
- Изъяны в проверке обновлений, проверке подписей и контрольных сумм выпусков или
  в официальном конвейере сборки и выпуска

Не входит в область действия:

- Уязвимости у провайдеров LLM или в их API; сообщайте о них самому провайдеру
- Проблемы, требующие уже скомпрометированного хоста, базы данных, учётной записи администратора или мастер-ключа
- Задокументированное поведение, например хранение учётных данных в незашифрованном виде, когда
  `GPROXY_MASTER_KEY` не задан, или запись содержимого запросов в соответствии с вашими
  настройками журналирования
- Небезопасные развёртывания, например открытие экземпляра в публичный интернет
  со слабыми или стандартными учётными данными администратора либо без TLS
- Отказ в обслуживании за счёт массового потока запросов или отсутствие ограничений частоты
  запросов без конкретных последствий для безопасности
- Отчёты автоматических сканеров без продемонстрированной возможности эксплуатации
- Вопросы об условиях обслуживания провайдеров или блокировке учётных записей
- Сторонние форки, зеркала и неофициальные сборки

Спасибо, что помогаете обеспечивать безопасность GPROXY и его пользователей.
