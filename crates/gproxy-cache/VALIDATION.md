# 验证记录

2026-09-17，gproxy 4.0 工作区，独立 crate 验证。

## 原生与真实 Redis

执行 `GPROXY_CACHE_REDIS_URL=redis://127.0.0.1:<临时端口> cargo test -p gproxy-cache
--all-features -- --include-ignored`，14 项集成测试全部通过，没有跳过 Redis 场景。

- Memory 7 项：字节值／CAS／过期版本、计数精度与固定窗口、许可／租约、32 路争抢、
  通知落后及取消等待、容量满不淘汰活跃锁／关闭订阅、独立实例隔离与参数预检。
- Redis 7 项：同一套五组契约跨两条独立客户端连接运行；另验证本地 TCP 转发断开后
  订阅明确关闭、操作不转为 Memory、重新订阅要求重新加载；以及不同 namespace/database
  的值和 Pub/Sub 通知均隔离。
- 并发验证：32 个任务竞争同一 CAS，只有 1 个胜者；计数上限 7，只有 7 次递增；许可
  上限 3，只有 3 个持有者。
- 精度覆盖完整 `i64::MAX`，包括超过 `2^53` 的增减、超限拒绝、下溢不修改数据。
- TTL 覆盖固定计数窗口不被后续递增延期、旧窗口不能扣减新窗口、短许可不缩短长许可、
  过期持有者不能释放或续期替代租约。

测试 Redis 为本机临时 Podman 容器 `redis:8-alpine`，实际服务端版本 8.10.1，redis-rs
为锁定的 1.7.0。使用临时回环地址端口、独立 namespace；没有访问生产 Redis，没有
FLUSHDB/FLUSHALL，也没有终止其他连接／容器。断网场景仅关闭测试自建的 TCP 转发器。
验证后删除临时容器。Valkey、TLS、Sentinel、Cluster、服务端故障切换未作实测；后两种
拓扑发现不在当前 adapter 支持范围内。

## 编译与静态检查

以下 Clippy all-targets 检查均以 `-D warnings` 通过：

- 原生 all-features。
- 原生 no-default-features（只保留公共契约）。
- 原生 no-default-features + redis。
- wasm32-unknown-unknown all-features（Memory，Redis TCP 实现不编入）。
- wasm32-unknown-unknown no-default-features。

crate 格式检查及 git diff 空白检查通过。没有为这次新增 crate 重跑无关的协议／UI 测试。

## WASM 执行

临时独立 harness 构建实际 WASM，在 Node 22.23.1 中运行 MemoryCache，没有启动 Tokio
runtime。字节读写、版本 CAS、完整 i64 计数、租约 TTL／旧 owner 拒绝，以及初始重新加载
通知与消息投递全部通过。该结果不表示 Memory 可以协调多个 Worker isolate。

harness 使用已装 CLI 对应的 wasm-bindgen 0.2.127；workspace WASM 检查使用 0.2.128。
临时源码、生成 WASM、构建目录验证后删除，契约测试保留在 crate 内。Core/manage 通知
接线与 Store 持久配置 revision 尚未实现，本轮没有声称完整多实例配置热更新已跑通。
