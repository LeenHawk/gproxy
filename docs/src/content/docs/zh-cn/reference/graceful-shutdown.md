---
title: 优雅关闭
description: native 关闭信号、请求 drain、stream settlement，以及当前不保证的行为。
---

GPROXY v2 native server 在顶层 HTTP server 外使用 Axum 的
`with_graceful_shutdown`。它监听：

- 所有 Tokio 支持的 native 平台上的 `Ctrl+C` / SIGINT；
- Unix 上的 SIGTERM。

收到任一信号后，server 停止接受新连接，并让 Axum 驱动正在执行的 service
future 完成。

## Native 流程

1. 二进制构建 persistence、cache、control-plane snapshot、HTTP client、channel registry 和 router。
2. server bind 配置的地址。
3. `axum::serve(...).with_graceful_shutdown(shutdown_signal())` 等待关闭信号。
4. 收到信号后，GPROXY 打印 `shutdown signal received`。
5. Axum graceful shutdown 停止接受新工作，并按 Axum/Tokio 行为等待 in-flight request future。
6. server future 完成后，`main` 返回。

当前 native serve 路径没有应用层固定 drain timeout。service manager 的
termination grace period 就是外层上限。除非可以接受中断正在处理的请求，否则不要发送 SIGKILL。

## Streams 与 billing settlement

HTTP server shutdown 与请求 settlement 是两层。对成功的 content-generation attempt：

- full response inline settle；
- native streaming response 会包一层 guard；
- 正常 stream 结束记为 `Complete`；
- 上游中断或客户端断开会通过 guard 记为 `Interrupted`；
- 包装后的 stream exactly-once settle。

settlement 会 refund pending quota estimate，并在能提取或计数 usage 时写入实际 usage/cost。如果进程在 settlement task 运行前被 kill，pending quota estimate 会通过 15 分钟 cache TTL 自愈，但被 kill 的请求可能不会产生最终 `usages` 行。

## 后台任务

serve 路径可能启动以下后台工作：

- 多实例配置 invalidation 的 Redis listener；
- usage 和请求日志 retention cleanup；
- instance settings 启用时的 tokenizer download 行为。

当前代码没有暴露单独 worker set，也没有文档化的应用层 worker drain deadline。
应把进程关闭理解为 HTTP graceful shutdown，加上 runtime drop 时对后台任务的 best-effort cancellation。

## 运维建议

- 优先使用 systemd、Docker、Kubernetes 或宿主 supervisor 发送 SIGTERM。
- 给进程合理的 grace window，让 in-flight 上游调用和 stream settlement 有机会完成。
- 不要为了 live config 修改而重启。大部分 provider、route、authz、pricing 和 rule 变更都应通过 console/admin API 完成，不需要重启。
- bind address、persistence backend、DSN、data dir、native cache backend、trusted proxies、CORS origins 或二进制升级这类进程级设置变更才需要重启。

## Edge 部署

wasm edge entry point 由平台按请求调用，不拥有长期运行的 Axum listener。
graceful shutdown 行为由 edge 平台控制，不由 native `shutdown_signal` 函数控制。
