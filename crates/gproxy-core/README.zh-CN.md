# gproxy-core

[English](README.md) | 简体中文

Provider 执行层的数据结构。上层先完成路由／模型别名解析、身份认证、策略判断和准入，
再调用 core。当前完成结构及进程内单调发布，尚未接入执行器和凭证选择器。

| 模块 | 数据结构 |
|---|---|
| `data` | Provider／Credential 执行快照、channel/client 引用、预编译改写匹配器 |
| `runtime` | 原子凭证版本、凭证亲和／轮换进度、健康状态、执行数据失效通知 |
| `context` | 已解析执行目标、不透明调用方 scope／session、请求／尝试／交互、用量报告 |

`CoreData` 只保存 providers、credentials、可复用 rewrite_rule_sets。路由、对外模型别名、
身份表、权限、OAuth client 白名单、订阅、计价和准入规则属于上层，路由亲和以及跨 Provider
负载均衡／失败转移也由上层完成。

`ExecutionTarget` 由上层传入单个 Provider、可选的已解析上游模型和明确允许的凭证集合。
后续执行器必须仅在该集合内轮换／重试；空集合不能回退为全局凭证池。上层同时给出最终
尝试预算和完成鉴权的不透明隔离 scope；core 不解析用户／key 权限。已绑定远程资源应传入
原目标和受限凭证，assignment 关联信息仅作归属记录，不代表可以迁移资源。

`Core<C>` 持有 `Arc<Store<C>>`、注入的 `Arc<dyn Cache>` 和 `ArcSwap<CoreData>`。
构造不做 I/O，`publish_snapshot` 只接受更新 revision 的已验证执行快照；请求持有自己的
Arc。持久 revision、数据组装和通知接线仍由上层完成。

`CredentialData` 保存执行配置、已解析的 client 和共享 `CredentialState`。解密 secret、
到期时间、Store version 整体原子发布，每次尝试固定一份版本。仅在 Store CAS 成功或
权威读取后发布，拒绝相同／旧版本。仍存在的凭证复用 slot，删除后摘除，重新创建用新 slot。
发布方法不隐式获取刷新租约或写库，刷新不需要替换整个执行配置快照。

CoreData 和解密凭证不实现 Debug／Serialize。session 由入站层提供，RequestFallback
不具有跨请求稳定性。请求、响应和流继续复用 protocol 类型；用量复用 channel 的
`NormalizedUsage`。一个请求可有多次尝试，一次尝试可产生多次真实交互；capture ID 属于
交互，不属于流片段。`UsageReport` 只回报观测用量，由上层记录日志、计价和结算，不携带
费用计算、订阅分摊或准入预占规则。

`RewriteTarget` 分为 Body（持有可选 JSON 路径）、Header（已校验的 HeaderName）和
Query（解码后的参数名称）。Header／Query 面向重复字段值，Query 仅限 request；
编译校验和实际修改尚未接入，见[改写设计](../../design/core-rewrite.md)。

```sh
cargo test -p gproxy-core
cargo clippy -p gproxy-core --all-targets -- -D warnings
cargo clippy -p gproxy-core --target wasm32-unknown-unknown --all-targets -- -D warnings
```

测试覆盖并发发布及保留在途视图，不表示选择器或网关已跑通。边界见
[crate 设计](../../design/crates.md)。
