---
title: 用量、日志与审计
description: GPROXY v4 记录什么：用量行与结算、下游与上游 capture、脱敏、审计轨迹，以及 HTTP 宿主还没暴露的东西。
---

下面的一切都写进数据库，因此共用一个数据库的每个实例共享同一份视图。

记录三样东西，而且它们是**运行期**分开开关的，不是编译期——运维者需要按部署来决定，而
Cargo feature 给不了这个。

| | 贵在哪 | 关掉的后果 |
| --- | --- | --- |
| 结算 | usage 提取、token 估算、定价 | 配额记账**和**成本统计一起失效 |
| capture | 复制 body，三者里最贵的 | 没有任何请求／响应留存 |
| tracing | 字符串格式化 | 只剩错误日志 |

开关是在干活**之前**问的，绝不是事后丢弃结果。v3 先复制 body 再让 sink 决定，所以空
sink 什么也省不下来；v4 先问，capture 关掉时不分配任何东西、不复制任何 body。

## 用量记录

每次结算的交换写一行。结算发生在**每一条**到达上游的路径上——没有绕过漏斗的快路径，
因为绕过它的路径就是未计量流量。

一行带着它的请求 id、模型、操作、结算后的费用，以及一份 `metrics` 文档，里面装着归一化的
token 计数、它们背后的按交换明细、结算状态和定价结果。

一个到达过两个 Provider 的请求在**一行里有两次交换**，各自带着那次尝试自己的 token 与
价格。因此按 Provider 分组是按交换而不是按行：一个发生过故障转移的请求在两边都计一次。

```json
{"dimensions":{"estimated":"true","unpriced":"true"},
 "exchanges":[{"attempt_id":"1a0c3372ac7-0-1","attempt_ordinal":1,
   "credential_id":"b8aad67f…","model":"gpt-4o-mini","provider_id":"5a45fd80…"}]}
```

有两个维度值得记住名字：

- **`estimated = true`**——上游没报 usage，GPROXY 自己数了 token。某个字段从未被报时它是
  `null` 而不是 `0`：上游没测量某样东西，不等于它测出了 0。
- **`unpriced = true`**——没有价格规则覆盖这个模型。请求照样按 0 结算，因为运维者要的是
  "有个模型在被白嫖"这个信号，而不是一个拒绝。

一个**被取消**的请求仍然是被计量的请求：它和别的一样得到自己的行，状态是 `cancelled`，
带着上游在被打断前设法报出的东西。

## 读回来

```sh
curl -s http://127.0.0.1:8787/portal/api/usage -H "Authorization: Bearer $GPROXY_KEY"
curl -s 'http://127.0.0.1:8787/portal/api/usage?groupBy=provider' -H "Authorization: Bearer $GPROXY_KEY"
```

```json
{"summary":{"requests":19,"inputTokens":43,"outputTokens":3830,
  "cachedInputTokens":0,"cacheCreationTokens":0,"reasoningTokens":0,
  "cost":"0","currency":null,"truncated":false,"scanned":19},
 "groups":[…],"trend":[]}
```

有已计价记录时 `currency` 为 `USD`，否则为 `null`。
固定 token 和媒体/工具数量使用专有列；`quantities` 用精确十进制字符串返回媒体、工具和
自定义数量。上游用量独立记录供应商、凭证和费用，不依赖日志。供应商和凭证筛选在 SQL 内完成。

聚合在 Rust 侧完成，默认最多扫描 50 000 条匹配的上游用量记录。
`truncated` 和 `scanned` 会说明是否触及上限，不把部分合计当成全部结果。

:::caution[运维者的读侧还没上 HTTP]
`gproxy-sdk` 有完整的查询侧——用量记录、汇总、分组、趋势、配额窗口与结算、请求日志及其
详情——桌面宿主通过 IPC 把它们全部暴露出来。**axum 宿主没有。** 没有
`GET /admin/api/usage`，也没有 `GET /admin/api/logs`，两者都回 404。

HTTP 宿主确实提供的是 `/portal/api/usage`、`/portal/api/quota` 和
`/portal/api/requests`——它们按构造就限定在调用方身份上——外加
`/admin/api/quotas/status` 和 `/admin/api/audit`。
:::

## 配额与预算

一个预算就是一行 metric 为 `cost`、unit 为 USD 的 `quotas`，挂在某个 owner 上——
`api_key`、`user`、`subscription`、`team` 或 `org`。

准入把调用方的**预算链**交给引擎：`[api_key?, user, subscription?, team?, org?]`，跳过
没设的那些。引擎对这些 kind **逐字比较**，不认识它们之间的任何层级关系，因此链上**任何**
owner 的**每一个**启用预算都适用，而且全部都要有余量。顺序是日志里报告的东西，不是优先级。

窗口**懒开**，用 `INSERT … ON CONFLICT DO NOTHING`，因此并发的实例收敛到同一行；过期的
窗口从不删除——下一个请求只是开下一个，而历史就是全部行。

```sh
curl -s 'http://127.0.0.1:8787/admin/api/quotas/status?owners=user:alice,team:t1' \
  -H "Authorization: Bearer $GPROXY_KEY"
curl -s -X POST http://127.0.0.1:8787/admin/api/quotas/{id}/reset \
  -H "Authorization: Bearer $GPROXY_KEY"
```

重置把打开的窗口关在*此刻*、给配额重新锚定、再开一个新的。历史保留。

**预算最多被超出一个请求。** 费用要等交换结束才知道，而这是不用预估、不用预扣、不用回滚
所付的代价。结算按请求 id 幂等，所以重放不会重复扣；结算失败丢的是记账，不是已经交付的
响应。

## Capture

一次 capture 有两侧，归两个主人。引擎为每一次物理发送写 `upstream` 那一行；只有宿主见得到
入站 HTTP 交换，所以 `downstream` 那一行是产品层的。

| | upstream | downstream |
| --- | --- | --- |
| 开关 | `enableUpstreamLog` | `enableDownstreamLog` |
| body 开关 | `enableUpstreamLogBody` | `enableDownstreamLogBody` |
| id | 它自己的，不透明 | **请求 id**，也是用量行的那一个 |
| body 存放 | 流式写进 capture 事件 | 内联列，缓冲且有上限 |
| WebSocket 帧 | 每帧一个事件 | 每帧一个事件，缓冲 |

```sh
curl -s -X PATCH http://127.0.0.1:8787/admin/api/settings \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"logging":{"enableUpstreamLogBody":true}}'
```

四个开关都是从**请求钉住的那个 revision** 的 settings 行上读的，因此一个请求不会在两份
配置下被记一半。

body 开关关闭时 body 不被复制，记录的 body 状态是 `notCaptured`——读的人正是靠这个来区分
"本来就没有 body"和"我们选择不留"。两行都是 0 字节但状态不同，说的是不同的事实。留下来的
每个方向上限 64 KiB，超过的按截断存放，状态写 `partial`。

**capture 失败绝不是请求失败。** 那时请求早已被应答，描述它时出的错不该变成一个响应。

### 一个套接字是一条记录加一串帧

一个 WebSocket 是**一条**记录，状态为 `101`，带一串跨两个方向的有序事件，而不是每次交换
一条记录。ping、pong 和 close 也被记录，而且保活帧留在它被发出的那一侧，因为它量的是它
走过的那条链路。

不写按轮次的记录，轮次列留空。轮次是某个方言的概念——OpenAI 的
`response.created`/`response.done`、Gemini Live 自己的——而宿主把 realtime 帧当不透明数据
转发。在这里发明一条 wire 没有画出的边界，等于往日志里放一条谁也没产生的记录。

### 重试是边，不是列

下游记录行上的 Provider、凭证和 metrics 列保持**未设置**。一个被重试的请求到达过两个
Provider、用过两把凭证，单独一列只能挑一个。边在不挑的前提下回答了这个问题，而计费用量
是那行用量记录。

上下游分别存入 `downstream_records` 和 `upstream_records`，通过
`capture_links` 表达多对多关系，也允许两侧独立存在。上游日志与用量写入各自保留关联，
因此未产生用量的失败调用仍能关联。`usage_records` 只保存上游实际用量；共享上游不复制用量，
删除日志也不删除用量。下游详情通过关联表返回上游用量列表，不落库下游汇总。

## 脱敏

脱敏发生在**写入时**，不是读取时。一个已经到达列里的密钥早就泄漏进了每一份备份和副本，
所以读侧什么也不脱敏，宿主也不能假设读的时候还有第二趟：如果它在数据库里，就是策略允许
它在那里。

标准清单上的 header 名、query 参数和 JSON 字段——`authorization`、`cookie`、`api_key`、
`access_token` 及其同类——会被遮蔽，而且一个请求的两侧遮蔽的是同一批东西。请求 body 在
被**截断之前**脱敏，因此长度永远不是绕过策略的办法。

不是 JSON 的 body 没有键可匹配，按原样存放。这也是 body 开关默认关闭的又一个理由。

`disableLogRedaction` 是显式的明文覆盖开关。

## 审计轨迹

每一个不是读的 `/admin/api` 调用写一行。动作由**匹配到的路由**推导，因此新路由不可能忘了
给自己命名：

```sh
curl -s 'http://127.0.0.1:8787/admin/api/audit?limit=4' -H "Authorization: Bearer $GPROXY_KEY"
```

```json
{"items":[{"id":"d052d378c1fc61c9fe4a4738d61fc996",
  "actorUserId":"5d1eb28a…","actorApiKeyId":"29371cab…","sourceIp":null,
  "action":"admin.settings.update","entityKind":null,"entityId":null,
  "outcome":"ok","detail":{"status":200},"createdAtMs":1789982332295}]}
```

读不被审计，这也正是两个刻意的披露动作——揭示凭证密钥、揭示 tokenizer 令牌——都是 `POST`
的原因。

这一行写在 revision 事务**之外**。这不是偷懒：一次失败的轨迹插入会把它只是在描述的那个
操作回滚掉，而一个*被拒绝*的操作——调查真正想要的那一行——根本没有事务可加入。写轨迹失败
只记日志，绝不向上传播。

调用方提供的 JSON 只能经由一个会脱敏的构造器进入条目，而失败记录的是错误的稳定 **code**，
绝不是它的消息，因为消息可能引用调用方发来的值。字段名在归一化大小写、下划线和连字符后
整体匹配，所以 `passwordPolicy` 和 `keyboardLayout` 活下来而 `password` 不会；命中的值被
**替换**成 `[redacted]` 而不是删掉——读的人于是能区分"这次操作带过密码"和"这次没有"。

查询按 `(createdAtMs DESC, id)` 从新到旧分页，所以同一毫秒写下的两行不会跨页重复或漏掉。

会话与审计行刻意**不**推进配置 revision。两者都不在身份快照里——认证每个请求都读会话
——所以推进它会让每次登录、每个被审计的操作，为一份快照里根本没有的变更让整个集群失效一次。

## 进程日志

```sh
gproxy serve --log-format json --log-filter 'gproxy=debug,info'
```

`--log-format` 是 `text` 或 `json`；`--log-filter` 用 `RUST_LOG` 语法，缺省回落到
`RUST_LOG`，再回落到 `info`。

日志走标准**错误**，而首次运行的管理员那一段走标准输出。这正是让密钥不进 journal 的原因，
也是让 `gproxy export --out -` 保持干净的原因。

## 请求 id

一个请求的 id 把它的用量行、capture 和边串起来。v4 **没有 `x-request-id` 响应 header**：
这个 id 被记录，不被返回。
