---
title: "快速开始"
description: "在控制台添加供应商和凭证，配置模型路由并发送第一个请求。"
---


先按[安装说明](/zh-cn/getting-started/installation/)启动 GPROXY。Application 用户完成首次设置后进入应用内控制台；CLI 和容器用户在浏览器打开 `http://127.0.0.1:8787/console/`，使用首次启动的管理员账户登录。

下面以 OpenAI 兼容服务为例。你需要一份可用的上游 API Key，以及该服务支持的模型名称。

## 1. 添加供应商

进入 **供应商**，新增一条连接：

- 名称填写 `openai-main`。
- 使用 OpenAI 官方服务时选择 `openai` 渠道。
- 使用其他 OpenAI 兼容服务时选择 `custom`，填写服务地址，并按上游文档选择支持的协议，例如 Chat Completions（`openai_chat`）。

供应商名称会用于直接调用的地址。不同供应商可以使用同一种渠道，分别保存各自的地址和凭证。

## 2. 添加凭证并测试

打开刚创建的供应商，在 **凭证** 标签中添加上游 API Key。使用 Codex、Claude Code 等登录渠道时，选择登录添加凭证，按页面提示完成授权。

在凭证行打开测试，选择上游支持的模型并发送一条简短消息。生成测试会真实调用上游，可能产生费用。成功后记下模型 ID，下一步会用到。

上游 API Key 保存在 GPROXY；客户端使用的是 GPROXY 签发的网关 API Key，两者不要混用。

## 3. 创建模型路由

进入 **模型路由**，新建名称为 `fast` 的路由，添加一个成员：供应商选择 `openai-main`，上游模型填写刚才测试成功的模型 ID，层级和权重先保留默认值。

客户端以后填写 `fast` 即可。更换上游时只需修改路由成员。一个供应商也可以直接调用，不必创建路由，见[模型与路由](/zh-cn/guides/models/)。

## 4. 发送请求

使用首次设置时保存的网关 API Key，或在 **我的账户 → 密钥** 创建一把新密钥。替换下面的占位值；如果修改过端口，也要修改地址。

```sh
export GPROXY_KEY='your-gproxy-api-key'
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"你好，请简单介绍自己。"}]}'
```

如果客户端要求填写 OpenAI Base URL，通常填 `http://127.0.0.1:8787/v1`；如果要求填写完整请求 URL，则填示例中的 `/v1/chat/completions` 地址。

其他协议和流式调用见[发送第一个请求](/zh-cn/getting-started/first-request/)。Codex CLI、Claude Code 等工具的专用配置见[CLI 客户端](/zh-cn/guides/cli-clients/)。

## 5. 查看用量

在控制台查看请求和用量记录，确认模型、供应商、token 数与费用。费用依赖配置的价格规则，不等于上游账单的实时余额。上游未报告的用量字段可能为空。

## 遇到问题

| 现象 | 先检查 |
| --- | --- |
| 无法连接 | GPROXY 是否运行，地址和端口是否正确 |
| `401` | 是否使用网关 API Key，密钥是否有效 |
| `403` | 当前用户或密钥是否有访问目标模型的权限 |
| `404 unknown_model` | `fast` 路由是否存在并启用，是否添加了成员 |
| `429` | 本地限额与上游额度，凭证是否暂时不可用 |
| 上游错误或不支持该操作 | 凭证测试结果、模型 ID、供应商协议与端点配置 |

需要用脚本管理时，[供应商](/zh-cn/guides/providers/)和[模型路由](/zh-cn/guides/models/)文档中提供了管理 API 示例。
