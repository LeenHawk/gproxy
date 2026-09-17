# gproxy-cache

[English](README.md) | 简体中文

独立的 TTL 状态、原子协调与失效通知库。不依赖 Store 实体、路由策略或数据库连接。
后端故障不会静默切换为本地 Memory。

| Feature | 后端 |
|---|---|
| `memory`（默认） | 进程内共享状态，支持原生与 WASM |
| `redis` | 原生异步 Redis／Valkey，TCP/TLS，独立订阅连接 |
| 不启用 feature | 仅公共契约，可自行实现 `Cache` |

```rust
use gproxy_cache::{Cache, MemoryCache, Replacement, CasOutcome};
use std::time::Duration;

let cache = MemoryCache::default();
let ttl = Duration::from_secs(3600);
let version = cache.put("route/session", b"member-a".to_vec(), ttl).await?;
let result = cache.compare_exchange(
    "route/session", Some(version),
    Some(Replacement { value: b"member-b".to_vec(), ttl }),
).await?;
assert!(matches!(result, CasOutcome::Applied(Some(_))));
```

启用 Redis 后调用 `RedisCache::connect(url, RedisOptions::new("deployment"))`。
clone 共享命令连接管理器；独立客户端只要连接相同 Redis database 和 namespace，就共享
状态。物理 key 和通知 channel 同时包含编码后的 namespace 与 database 编号，避免 Redis
Pub/Sub 不随 SELECT 隔离导致跨库串通知。key/topic 编码不依赖用户输入的分隔符。
当前连接单个 Redis endpoint，不实现 Sentinel 发现或 Cluster 重定向。需要 scripts、hash、
sorted set、expiry、TIME 和 Pub/Sub 权限。本次实测 Redis 8；Valkey 和 TLS 尚未进行实库验证。

## 操作契约

| 操作 | 语义 |
|---|---|
| `get / put / delete` | 不透明字节值、必须有正 TTL，每次写入产生新版本 |
| `compare_exchange` | 按版本或不存在条件替换／删除，替换时产生新版本和 TTL |
| `counter / increment` | 非负整数，完整支持到 `i64::MAX`；检查上限与递增原子完成 |
| `decrement` | 检查计数窗口代号再减；下溢失败且不修改状态 |
| `acquire_permit` | 并发许可，每个持有者单独到期 |
| `acquire_lease` | 同一许可域中 limit=1 的简化入口，可用于凭证刷新协调 |
| `renew_permit / release_permit` | 仅匹配且未过期的持有者可续期／释放 |
| `publish / subscribe` | 上层定义的字节通知，只作为可丢失的失效提示 |

KV、counter、permit、topic 属于四个独立域，同一逻辑 key 可分别使用。每次操作只保证
单 key 原子性，没有跨 key 事务。空字节值合法；空／超限 key、超限 payload、非法 TTL
返回错误。TTL 向上取整到毫秒，并限制在可精确表示的范围内（约 14.2 万年）。Memory 用
单调时钟，Redis 到期及许可使用 Redis 服务端时间。

计数器是**固定窗口**：首次创建设置 TTL，后续加减不延长，减到零也保留原窗口。
新窗口有新 generation，旧请求不能减到新窗口。decrement 本身没有幂等回执，调用方必须
保证每笔只释放一次，并处理提交结果不明确的情况。最终费用结算与幂等账本仍归 Store。
Redis Lua 使用十进制字符串比较及 HGET 返回最终值，超过 `2^53` 也不经过浮点数计算。

同一资源的许可 limit 必须由各实例保持一致。不会自动续期或在 handle 丢弃时自动释放。
过期持有者不能复活许可，短 TTL 持有者不会缩短长 TTL 持有者的有效期。失去许可后，
调用方应停止受保护工作。随机 owner token 防止旧持有者续期／释放，但不是单调 fencing
编号，也不能阻止已经发出去的远程副作用；持久写入仍配合 Store 版本／CAS。Redis 故障
切换或淘汰可能丢失协调状态，部署应采用合适的 no-eviction／持久化配置；这里不是共识锁。

派发后的超时、断线或取消可能已经提交。库不盲目重试失败写入。redis-rs 会为后续调用
重连；NOSCRIPT 可以加载脚本，因为这次被拒绝的脚本尚未执行。重试非幂等操作前检查
状态，或依赖持久操作回执。

## 多实例通知

先 `subscribe("config")`，拿到的第一个事件是 `Notification::ResyncRequired`：此时订阅
已就绪，调用方再从数据库一致地加载配置和版本。之后 `publish` 发送上层定义的通知。
慢消费者超过有界投递队列容量时会再次收到 `ResyncRequired`。Redis 断线明确关闭订阅，
上层需要重新订阅并重新加载；丢弃订阅会中止读取任务，`recv` 支持取消后重试。
Memory 通知只在同一个 cache 的 clone 间共享，独立实例／进程不互通；WASM Memory
仅当前 isolate 有效。

不提供事件重放、确认、恰好一次交付，也不保证数据库提交与 publish 原子。没有订阅者时
publish 仍成功。提交后、发布前进程可能退出，所以消费者仍须定期检查数据库中的配置
版本。配置修改与版本递增须在 Store 同一事务中完成，加载实体与版本也须保持一致。
版本字段、Core/manage 接线、快照编译和通知 payload 属于上层，尚未由这个 crate 实现。
详见 [design/cache.md](../../design/cache.md)。通知不携带解密凭证或 bearer token。

## 容量与运行边界

Memory 默认 10,000 个有效 key、256 个活跃 topic。共享 Limits 默认 key 上限 1,024 bytes、
单 value/message 1 MiB、每个资源 10,000 个许可、投递队列 256 条消息。这是数量／单项
限制，不是分配器级总内存预算，须按工作负载配置。Redis 同样检查单次操作限制，服务端
容量由 Redis 管理；同一 namespace 的客户端须保持限制配置一致。

Memory 不淘汰有效值、计数器或租约；满时先回收过期项，仍满则返回 `Capacity`。
读取时始终检查到期；可显式调用 `purge_expired()` 回收冷的过期数据。没有后台清理线程，
Memory 操作不要求 Tokio runtime，只有通知等待复用 Tokio sync，由宿主驱动 future。

## 验证

```sh
cargo test -p gproxy-cache
cargo clippy -p gproxy-cache --all-features --all-targets -- -D warnings
cargo clippy -p gproxy-cache --target wasm32-unknown-unknown --all-features --all-targets -- -D warnings
GPROXY_CACHE_REDIS_URL=redis://127.0.0.1:6379 cargo test -p gproxy-cache --all-features -- --include-ignored
```

Redis 测试默认显式 ignored，需提供测试 Redis 才执行；断线测试需要无鉴权 TCP endpoint，database 隔离测试要求至少两个 database。
测试使用独立 namespace，不执行 FLUSHDB／FLUSHALL，也不杀服务端客户端。断线只关闭
测试自己的本地转发连接。已执行的验证见 [VALIDATION.md](VALIDATION.md)。
