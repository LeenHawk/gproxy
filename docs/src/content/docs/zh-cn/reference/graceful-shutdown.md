---
title: 优雅关机
description: gproxy 如何 drain worker、flush 用量并干净退出。
---

优雅关机由
[`apps/gproxy/src/main.rs`](https://github.com/LeenHawk/gproxy/blob/main/apps/gproxy/src/main.rs)
和
[`apps/gproxy/src/workers/mod.rs`](https://github.com/LeenHawk/gproxy/blob/main/apps/gproxy/src/workers/mod.rs)
共同实现。目标很简单：收到 `SIGINT` / `SIGTERM` 后，停止接收新请求、让在途请求完成、
刷新后台 sink，然后在有限的时间内退出。

## 关机顺序

1. 进程监听 `Ctrl+C`，Unix 下同时监听 `SIGTERM`。
2. 触发关机后，Axum 服务进入 `with_graceful_shutdown`，**停止接收新请求**。
3. 主线程调用 `worker_set.shutdown()`，向所有后台 worker 广播关机信号。
4. `WorkerSet` 最多等待 **5 秒** 让 worker 完成收尾。
5. `UsageSink` 关闭接收端，排空剩余用量消息，执行**最后一次批量写入**。
6. `HealthBroadcaster` 把仍在 debounce 窗口内的健康状态刷到数据库。
7. `QuotaReconciler` 和 `RateLimitGC` 在下一次循环迭代时收到信号后退出。
8. 若 5 秒内仍有 worker 未完成，进程会打印警告但**不会**无限阻塞 ——
   直接退出，避免卡死编排系统。

## 运维注意事项

- **设置合理的 terminationGracePeriodSeconds。** Kubernetes 或 systemd 下，
  至少给 gproxy 10 秒时间退出 (5 秒 worker drain + 余量)。太短可能会截断
  最后一批用量写入。
- **热更新 != 重启。** 绝大多数运行态设置 (供应商、模型、用户、权限、
  限流、配额) 都可以在控制台或管理 API 中热改，下一次请求即生效。
  只有进程级变更 (监听地址、数据库 DSN、升级二进制) 才需要重启。
- **请勿故意 SIGKILL。** 强制杀进程会跳过用量 drain，最后一批请求
  不会出现在 `usages` 里，也不会计入 `cost_used`，直到下一次
  `QuotaReconciler` 重算。

## Worker 速查

| Worker | 关机时的动作 |
| --- | --- |
| `UsageSink` | 关闭接收端，drain 队列，最后一次批量写入。 |
| `HealthBroadcaster` | Flush debounce 中的健康状态。 |
| `QuotaReconciler` | 下一次循环迭代退出。 |
| `RateLimitGC` | 下一次循环迭代退出。 |
