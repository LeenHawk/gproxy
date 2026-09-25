---
title: GPROXY 是什么?
description: GPROXY v4 做什么、给谁用、一个请求如何流经它，以及本站其他页面都会用到的词汇。
---

**GPROXY** 是一个自托管的 LLM API 网关。客户端只面对一个 base URL 和一把 API key。
GPROXY 判断调用方是谁、可以触达什么，把模型名解析到某个上游 Provider 和它的一把凭证，
在客户端与上游线格式不同时做协议转换，扣减调用方的预算，并记录这次交换的花费。

v4 是一次重写。要做的事和 v3 一样，形态不一样：引擎现在是一个不含 server 的可嵌入库，
产品规则在第二个不含传输层的库里，而**宿主**就是把两者接到 socket 上的那一层薄壳。
一共三个宿主，它们共享同一张路由表和同一套判断。

## 给谁用

- 手里有多个上游账号、希望把它们汇成一个端点的运维者，需要故障转移和按凭证的健康跟踪。
- 希望业务代码只依赖一个模型名、而不是某家厂商 SDK 的团队。
- 希望 Codex CLI 或 Claude Code 跑在共享凭证上、且每个请求都被计量的用户。
- 不想跑网关、但想要同样凭证纪律的 Rust 应用，见[嵌入核心库](/zh-cn/reference/embedding/)。

## 接受的线格式

GPROXY 接受 OpenAI Responses、OpenAI Chat Completions、Claude Messages 和
Gemini GenerateContent。其中任何一种都可以由讲另一种的上游承接：转换是**两种格式之间
的直接映射，没有中间表示**。上游规范的变化速度快过任何中枢格式能跟上的速度，而转换
保真度正是这个产品本身。

一个由 OpenAI Chat 上游承接的 Claude Messages 请求，回来的仍然是 Claude Messages，
usage 也在：

```json
{"type":"message","id":"chatcmpl-…","content":[{"type":"text","text":"…"}],
 "model":"gpt-4o-mini","role":"assistant","stop_reason":"end_turn",
 "usage":{"input_tokens":11,"output_tokens":7}}
```

内容生成之外，同一条路径还承载 embedding、rerank、图像、音频、视频、文件、token 计数、
模型列表、conversation、moderation 和 realtime 套接字。流式按调用方要求的分帧交付：
SSE、WebSocket，或 Gemini 的增量 JSON 数组。

## 一个请求怎么走

```text
传输层（宿主）
  → 认证    Caller   { 用户、角色、API key、组织、团队、订阅 }
  → 准入    Admitted { 允许的 Provider、允许的凭证、预算链、
                       scope、会话标识、限流许可 }
  → 引擎    解析模型、走完 Plan、转换、发送、结算
```

每一步都在收窄，且**下层绝不重新打开上层收窄过的东西**。引擎拿到的是一个 Provider 集合
和一个凭证集合，而不是一个待解释的调用方，因此它无法重新回答上层已经回答过的问题。

准入按固定顺序执行：权限、凭证可见性、预算链、scope、会话标识，最后是限流。限流放在
最后，因为它是唯一会*消耗*东西的一步：一个因权限被拒的请求不应该推动共享计数器。

## 三种寻址方式

同一个实例在三个挂载点上提供同一套上游 API。

| 路径 | 挂载点 | 收窄到 |
| --- | --- | --- |
| `/v1/messages` | 聚合 | 不收窄 |
| `/acme/v1/messages` | namespace `acme` | `acme/` 下的公开名称 |
| `/openai-prod/v1/messages` | Provider `openai-prod` | 那一个 Provider |

**namespace** 是带斜杠的公开模型名的第一段，因此公开 `acme/fast` 就产生了 namespace
`acme`。**Provider** 挂载点用的是 `providers.name`——运维者取的标签，而不是行 id，
因为没有人会把机器生成的 id 敲进客户端的 base URL。同名时 namespace 胜过 Provider。

**只有在剥掉前缀后剩下的部分也是本网关提供的 surface 时，前缀才会被剥掉。** 因此有歧义
的路径一律倒向聚合挂载点：丢掉一个挂载点是运维者看得见的 404，而错认一个挂载点则是把
请求发给了错误的上游。

## 核心概念

| 概念 | 含义 |
| --- | --- |
| 渠道（Channel） | 编译进二进制的某一族上游适配器：URL、凭证怎么注入、讲哪些方言、流里怎么报 usage、怎么登录和刷新。共 25 个，从 `openai`、`claudeapi` 到 `codex`、`kiro`、`custom`。 |
| Provider | 某个渠道上的一条已保存连接：名字、可选 base URL、该渠道自己的 `config` JSON，以及一个凭证池。 |
| 凭证（Credential） | 池中的一份密钥——API key、OAuth 令牌对、会话 cookie、服务账号材料——静态加密存放，带生命周期状态和归属。 |
| 模型路由（Route） | 名称就是客户端请求的模型名，下含一组有序成员，每个成员是一个 Provider 加一个上游模型，带 `tier`（故障转移层级）和 `weight`（层内分流）。 |
| 网关 API key | 客户端出示的东西。它属于某个用户，可绑定到组织、团队和订阅；这个绑定决定它能触达哪些凭证、花谁的预算。 |
| 改写规则 | 对 body 文本、指定 header 值或指定 query 值的一条有序正则替换，可按操作、模型、入站 header 或流事件过滤。 |
| 操作规则 | 对某个操作上渠道行为的按 Provider 覆盖。渠道默认值留在代码里。 |

## 三个宿主

| 宿主 | 是什么 |
| --- | --- |
| `gproxy` | 原生 server：产品层之上的一个 axum router，跑在 socket 上。运维者跑的就是它。 |
| `gproxy-desktop` | 同一个实例之上的 Tauri 窗口，另开一个只提供数据面的 `127.0.0.1` HTTP socket 给不会说 IPC 的 CLI。**v4 新增。** |
| `gproxy-host-edge` | 一个 Cloudflare Workers `fetch` handler，挂载的是**同一个** axum router。**v4 新增。** |

Workers 宿主没有自己的路由表：原生宿主新增的路由就是它提供的路由，那边一行都不用改。
见 [边缘部署（Cloudflare Workers）](/zh-cn/deployment/edge/)。

## GPROXY 不做什么

GPROXY 不托管模型、不做推理。它也不是通用反向代理：它解析 LLM 请求体、改写流、提取
token 用量、管理各家厂商特有的认证。它默认绑定 `127.0.0.1`；把它暴露出去、备份数据目录、
保管主密钥，都是你的事。

另外目前**还没有发布流水线**。v4 从源码构建——没有安装包、没有发布的容器镜像、也没有
签名产物。见[安装](/zh-cn/getting-started/installation/)。

## 下一步

- [安装](/zh-cn/getting-started/installation/)与[快速开始](/zh-cn/getting-started/quick-start/)。
- [架构](/zh-cn/introduction/architecture/)：crate 与各处接缝。
- [Provider 与凭证](/zh-cn/guides/providers/)和
  [模型、路由与公开名称](/zh-cn/guides/models/)。
