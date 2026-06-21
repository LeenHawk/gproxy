# Backend 实施计划归档 / Backend Implementation Plan Archive

> 状态：历史归档，不再作为当前实现规范使用。
> Status: historical archive; no longer the active implementation specification.

## 中文

这页原本是 v2 后端早期的 M1-M10 实施规格，包含大量 trait 草案、文件拆分计划、
集成顺序和 smoke test 计划。当前代码已经越过这个阶段：仓库现在是一个单 crate，
同时支持 native binary、edge wasm、嵌入式 console、控制面快照、v1 迁移、agent
channel、rule set、billing、quota、observability 和 self-update。

因此，本页不再保留旧的逐任务实现草案。继续把它当作规范会产生两个问题：

- 文件路径和模块职责已经变化，旧计划容易误导读者。
- 当前设计边界应以代码和新的架构文档为准，而不是以早期任务拆分为准。

当前应参考：

| 主题 | 当前文档 |
| --- | --- |
| 系统架构、模块边界、请求生命周期 | `docs/architecture-design.md` |
| 日常开发命令、feature、CI/release | `docs/developers/README.md` |
| console 与 API 的部署关系 | `docs/deployment.md` |
| edge wasm 平台部署 | `docs/edge-deploy.md` |
| v1 SQLite 迁移 | `docs/v1-to-v2-migration.md` |
| 通用 transform rule 的未定稿设计问题 | `docs/generic-transform-rule-design-notes.md` |

历史上，本计划确立过几个仍然有效的方向：

- channel 是上游适配边界，不负责跨协议 transform。
- pipeline 按阶段拆分，请求热路径读控制面快照。
- native 和 edge 共用核心处理逻辑，只在 HTTP/runtime 边界分叉。
- 后端变更应优先保持模块小、边界清晰、失败可观测。
- provider-specific 规则优先变成配置/预设，而不是不断增加后端专用 rule kind。

如果需要恢复旧计划的细节，请从 git 历史查看本文件旧版本，而不要把归档内容复制回当前
文档。当前实现的事实来源是 `src/`、`console/`、`deploy/` 和上表列出的重写文档。

## English

This page used to be the early M1-M10 backend implementation specification for
v2. It contained trait drafts, file-splitting plans, integration order, and
smoke-test plans. The codebase has moved past that phase. The repository is now
a single crate supporting native binary, edge wasm, embedded console,
control-plane snapshots, v1 migration, agent channels, rule sets, billing,
quota, observability, and self-update.

The old task-level implementation draft is intentionally no longer kept here.
Treating it as the current spec would be misleading because:

- file paths and module responsibilities have changed;
- current design boundaries should come from code and the rewritten architecture
  docs, not from the early task breakdown.

Use these current documents instead:

| Topic | Current document |
| --- | --- |
| System architecture, module boundaries, request lifecycle | `docs/architecture-design.md` |
| Developer commands, features, CI/release | `docs/developers/README.md` |
| Console/API deployment relationship | `docs/deployment.md` |
| Edge wasm platform deployment | `docs/edge-deploy.md` |
| v1 SQLite migration | `docs/v1-to-v2-migration.md` |
| Unresolved generic transform-rule design questions | `docs/generic-transform-rule-design-notes.md` |

The old plan established several directions that remain valid:

- channels are upstream adapters and do not own cross-protocol transforms;
- the pipeline is split into phases, and the hot path reads control-plane
  snapshots;
- native and edge share core processing and diverge only at HTTP/runtime
  boundaries;
- backend changes should keep modules small, boundaries clear, and failures
  observable;
- provider-specific behavior should prefer configuration/presets over new
  backend-only rule kinds.

If you need details from the original plan, inspect this file in git history
instead of copying archive material back into the current docs. The current
sources of truth are `src/`, `console/`, `deploy/`, and the rewritten documents
listed above.
