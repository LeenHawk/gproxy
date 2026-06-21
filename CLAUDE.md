# gproxy — 项目工程规范

> 项目级指令,优先级高于任何工具/skill 默认行为。

## 测试
- **不使用 TDD**。不做测试先行,不写大量冗余测试,**避免过度测试**。
- 仅对真正棘手的逻辑、真实 bug 的回归点写**精简**测试。

## 代码组织
- 单文件理想 **≤200 行**,**绝对不超过 500 行**;确需超过先经人工审批。
- 按职责结构化拆分模块;文件变大即视为"做了太多事"的信号。

## 写代码前
- 先搜索代码库是否已有部分实现可复用/扩展,避免重复实现。
- 评估是否需要先做小范围重构,让新代码干净落地(重构保持最小范围)。

## 写完代码后
- 收尾必跑 `cargo fmt` 与 `cargo clippy`;clippy 告警要修而非压制(除非有明确理由)。

## Git
- 提交信息**不加** AI co-author 行。

## 部署产物 / `deploy` 分支
- 边缘平台(Cloudflare / Netlify / Supabase / EdgeOne / Appwrite-deno …)
  的**预构建产物**(wasm + wasm-bindgen glue + 入口 + 平台配置,**即点即部署**)
  放在孤儿分支 **`deploy`** —— **只放产物,不放 Rust 源码**。
- README 的**一键部署按钮指向 `deploy` 分支的子目录**(各平台构建环境没有 cargo,
  无法从源码现编,只能直接发预构建产物)。
- 各平台的**源码**(`build.sh`、入口 `main.ts`/`index.ts`/`worker.js`、配置)留在
  主分支 `deploy/<平台>/`;生成的 glue 是 gitignore 的构建产物。
- `deploy` 分支由发布流程在每次 release 时**刷新**(与对应 tag 一致),不手工维护。

## 分支保留策略
- **长期保留**:`main`、`staging`、`deploy`,以及所有 release `tags`。
- 其它(feature / 工作)分支:合并后 **squash 并删除**,不长期保留 —— 历史靠 tags
  与 squash 提交承载,保持分支列表干净。

## 架构
- v2 架构设计见 `docs/architecture-design.md`。
