---
title: 快速开始
description: "从下载的二进制到可用网关：启动 gproxy、创建管理员、添加 Provider 与路由、签发密钥并发送请求。"
---

本页把一个全新的原生安装带到第一个成功的请求。假设你使用的是[下载](/zh-cn/getting-started/downloads/)
页上的便携压缩包；安装包会替你完成第 1、2 步并打开控制台。

## 1. 启动 gproxy

```bash
chmod +x ./gproxy
./gproxy
```

服务监听 `127.0.0.1:8787`，创建 `./data/gproxy.db`，并输出 `GPROXY listening`。
`gproxy --help` 列出全部参数。每个参数都有对应的 `GPROXY_*` 环境变量，两者都可以
写进工作目录或数据目录下的 `.env` 文件。优先级依次为参数、环境变量、`./.env`、
`<数据目录>/.env`、默认值。

一个最小的 `.env`：

```env
GPROXY_HOST=127.0.0.1
GPROXY_PORT=8787
GPROXY_DATA_DIR=./data
GPROXY_MASTER_KEY=<标准 base64，32 字节>
```

用 `openssl rand -base64 32` 生成密钥。不设置时，凭证和用户密钥以明文保存。请在
添加第一个凭证之前设置它；之后再改属于轮换操作，见[配置](/zh-cn/reference/configuration/)。
安装包会替你写好带有生成密钥的 `.env`。

容器方式：

```bash
docker run -d --name gproxy -p 8787:8787 \
  -v gproxy-data:/var/lib/gproxy \
  ghcr.io/leenhawk/gproxy:<tag>
```

## 2. 创建管理员

打开 <http://127.0.0.1:8787/admin>。全新存储上控制台显示**创建管理员**。填写用户名
和密码，提交后即完成登录。控制台导航依次为概览、供应商、负载均衡、规则、身份、统
计、定价、分词器词表、更新和设置。

管理员也可以通过 `GPROXY_ADMIN_PASSWORD` 创建，见[安装](/zh-cn/getting-started/installation/#首次启动)。

## 3. 添加 Provider 和凭证

进入**供应商 → 添加供应商**。填写路由名（这个 Provider 的稳定标识，也可用作命名模
式的路径前缀），选择通道，并选择凭证策略：**轮询**在凭证池中轮转请求，**按 API 密
钥粘滞**让每个客户端密钥固定使用一个凭证。通道决定显示哪些设置，例如 `custom` 的
Base URL 或 `aws-bedrock` 的区域。保存 Provider 时会预置该通道的路由规则，并为它
创建一个名为 `<provider> · defaults` 的空私有规则集。

然后打开这个 Provider，选择**添加凭据**。提供密钥有两种方式：

- **直接粘贴。** 选择凭证类型（API key、OAuth 或 Cookie），填写通道声明的字段，或
  在 JSON 字段中填入原始凭证对象。标签可选，默认从密钥推导。
- **登录。** 声明了登录方式的通道会显示**登录方式**选择器。`codex` 提供**浏览器登
  录**（授权码加 PKCE）和**设备代码**；`claudecode` 提供**浏览器登录**和**浏览器
  Cookie**。开始登录，在浏览器中批准，粘贴回调 URL（或在验证页输入设备代码），然
  后完成登录。token 加密保存，由 GPROXY 在独占租约下自行刷新。

每一行凭证都带有流量权重、可选的每分钟请求数和每分钟 token 数限制、代理覆盖，以
及观测到的健康状态。

可以选择打开 Provider 的**模型**标签，用**从上游拉取**记录它提供的模型 id，连同能
力和默认价格。

## 4. 创建路由

进入**负载均衡 → 新建负载均衡**。填写路由名和最大尝试次数（首次尝试加故障转移次
数）。然后**添加成员**：选择 Provider，输入上游模型 id，可选固定某个凭证，并设置故
障转移层级和权重。第 0 层用尽后第 1 层才会接到流量；权重在同层健康成员之间分流。
再从其他 Provider 添加成员用于故障转移。

创建负载均衡并不会把它对外公开。在**模型映射**下添加一个指向它的公开模型名；客
户端在 `model` 中发送的就是这个名字。聚合解析依次经过别名、变体后缀、公开模型
名，再到负载均衡的成员。路由名本身只能通过命名前缀 `/{route}/v1/...` 访问。

## 5. 创建用户和 API 密钥

进入**身份**并创建用户。密码可选，只有需要登录门户时才用到。然后在该用户的
**API 密钥**下选择**创建 API 密钥**：填写标签，选择前缀——**标准密钥 (sk-)** 供 API
客户端使用，**Codex 密钥 (at-)** 供 Codex CLI 的 access-token 登录使用——以及可选的
过期时间。密钥显示时立即复制。之后列表只显示前缀；显示完整密钥是一项单独的、会被
审计的操作。

权限默认拒绝。在**访问**下添加一条效果为**允许**的权限，可以针对所有 Provider 或某
一个，针对所有操作或某一个操作组。它可以挂在密钥、用户、团队或组织上并向下继承。
没有任何允许权限时，该密钥的每个请求都会被 `403` 拒绝。限流和成本配额也在同一处
添加。

## 6. 发送请求

把占位符替换为密钥和公开模型名：

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "<public-model-name>",
    "messages": [
      { "role": "user", "content": "Say hello in one short sentence." }
    ]
  }'
```

响应带有 `x-request-id` 头。在控制台打开**统计 → 请求审计**，可以看到这个请求和
它产生的上游调用。

## 下一步

- [发送第一个请求](/zh-cn/getting-started/first-request/)展示同一调用在每种接受
  格式下的写法、流式、模型列表和命名前缀。
- 设置了密码的用户可以登录 `/portal` 创建自己的密钥，并复制 curl、OpenAI、Claude、
  Gemini SDK、Codex CLI 和 Claude Code 的连接片段。见[控制台、门户与公开站点](/zh-cn/guides/console/)。
- [CLI 客户端](/zh-cn/guides/cli-clients/)介绍如何把 Codex CLI 和 Claude Code 指向
  网关。
