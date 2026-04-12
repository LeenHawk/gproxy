---
title: 模型与别名
description: GPROXY 如何端到端地解析一个模型名 —— 包括别名、重写规则和本地 model_list 分发。
---

发给 GPROXY 的每个请求都携带一个模型名。它从这个字符串走到真实上游调用的路径
只有**一条规范管道**：

```text
permission  →  rewrite  →  alias  →  execute
```

每一阶段要么让请求原样通过，要么把目标 `(provider, model_id)` 改写后交给下一阶段。
这四个阶段都运行在 handler 层 —— SDK 里的 `GproxyEngine::execute` 只负责最终的
上游调用。

## 统一的 `models` 表

自 v1.0.5 起，真实模型和别名共用同一张 `models` 表，只靠一列来区分：

- `alias_of = NULL` —— 真实模型条目，带 provider、model id 和可选的定价。
- `alias_of = Some(id)` —— 别名，指向同一张表里另一行的 id。

管理 API 和控制台读写的就是这张表。引擎在启动和 reload 时从它重建内存中的
别名查找结构 (`HashMap<String, ModelAliasTarget>`)。

## 定义别名

种子 TOML 中：

```toml
[[model_aliases]]
alias = "chat-default"
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
enabled = true
```

运行时使用内嵌控制台供应商工作区中的*模型*标签。种子中的
`[[model_aliases]]` 会在启动时被导入到统一的 `models` 表。

## 请求时的别名解析

当客户端发送 `"model": "chat-default"` 时，管道依次：

1. **权限检查** —— 用户是否有任何 `model_pattern` 匹配该别名名的权限？
2. **重写规则** —— 通道级重写规则可在别名查找之前把别名改写成另一个字符串。
3. **别名解析** —— 从统一的 `models` 表查出别名，解析为具体的
   `(provider, model_id)` 对。
4. **执行** —— 引擎用解析结果准备并发起上游请求。

非流式响应的 `"model"` 字段会被改写回客户端发送的别名；流式 chunk 也在引擎里
逐 chunk 改写。客户端感知不到这是别名 —— 只是计费和路由走向了另一个真实模型。

## 拉取上游模型

控制台*模型*标签页提供一个 **Pull Models** 按钮，对应
`POST /admin/models/pull`。该端点会调用上游真实的 `model_list` 并返回 id 列表，
控制台把这些条目作为真实模型 (`alias_of = NULL`) 导入本地 `models` 表，
价格留空供管理员后续编辑。

这样你就能获得"从上游拉取、本地自定义"的工作流，而不必手动编辑 TOML。

## `model_list` / `model_get` 分发

模型列表接口的行为取决于路由所使用的分发模板：

- **`*-only` 预设** (`chat-completions-only`、`response-only`、`claude-only`、
  `gemini-only`) 的 `model_list` 和 `model_get` 默认使用 **Local** 实现：
  请求完全由本地 `models` 表响应，永远不会打到上游。
- **`*-like` / 透传预设**仍会为 `model_list` 调用上游，但 GPROXY 会把上游响应
  与本地 `models` 表**合并**：本地存在但上游没有的真实模型追加到响应里，别名镜像
  其目标模型。`model_get` 先查本地表，未命中再落到上游。

`GproxyEngine::is_local_dispatch(...)` 让 handler 在调用 `engine.execute` 之前
先判断是否走本地分发。

## 定价与别名

记账时会**先按别名名**查询价格，未命中时再回落到解析出的真实模型名。
这意味着管理员可以为同一个真实模型在不同别名上设置不同价格 —— 比如给
`premium-gpt4` 别名加价，同时让 `chat-default` 维持成本价。

完整的 `ModelPrice` 形状、billing mode 的选择、以及工具调用的次数计费逻
辑，见[价格与工具计费](/zh-cn/reference/pricing/)。
