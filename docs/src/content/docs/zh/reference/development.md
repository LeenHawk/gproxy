---
title: 开发与测试
description: 本地开发、测试、目录与常用命令参考。
---

## 常用命令

后端：

```bash
cargo fmt
cargo check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p gproxy
```

前端：

```bash
cd frontend/console
pnpm install
pnpm typecheck
pnpm build
```

## Provider 回归脚本

仓库内置脚本：

- `tests/provider/curl_provider.sh`
- `tests/provider/run_channel_regression.sh`

示例：

```bash
API_KEY='<key>' tests/provider/curl_provider.sh \
  --provider openai \
  --method openai_chat \
  --model gpt-4.1
```

```bash
API_KEY='<key>' tests/provider/run_channel_regression.sh \
  --provider openai \
  --model gpt-5-nano \
  --embedding-model text-embedding-3-small
```

## 新增原生渠道贡献指南

如果 custom 模式无法满足你的上游需求（例如自定义鉴权流程、特殊路径规则、定制响应归一化、或更深层转换能力），建议贡献一个原生渠道实现。

### 后端改造清单

1. 在 `sdk/gproxy-provider/src/channels/<your_channel>/` 新建渠道模块。
2. 按需实现：`settings.rs`、`credential.rs`、`dispatch.rs`、`upstream.rs`、`mod.rs`。
3. 在 `sdk/gproxy-provider/src/channel.rs` 注册渠道枚举和字符串 id。
4. 在 `sdk/gproxy-provider/src/registry.rs` 注册渠道能力元信息和默认 dispatch。
5. 在 `sdk/gproxy-provider/src/settings.rs` 接入 settings 解析/序列化。
6. 在 `sdk/gproxy-provider/src/provider.rs` 接入渠道执行分发。
7. 若支持 OAuth 或上游 usage，同步接入对应 runtime 分支。

### 管理端前端改造清单

1. 在 `frontend/console/src/modules/admin/providers/channels/<your_channel>/` 增加渠道文件。
2. 在前端渠道注册表中挂载该渠道，确保管理端可配置。

### 验证与回归

```bash
cargo check
cargo test --workspace
```

```bash
tests/provider/curl_provider.sh
tests/provider/run_channel_regression.sh
```

## 数据目录

默认路径：

- 数据目录：`./data`
- 默认数据库：`sqlite://./data/gproxy.db?mode=rwc`
- tokenizer 缓存：`./data/tokenizers`

`dsn` 可切换到 mysql/postgres。
