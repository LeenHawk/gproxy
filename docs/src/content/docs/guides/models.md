---
title: Models & Aliases
description: How GPROXY resolves a model name end-to-end, including aliases, rewrite rules, and local model-list dispatch.
---

Every request to GPROXY carries a model name. The route it takes from that
string to an actual upstream call is **one canonical pipeline**:

```text
permission  →  rewrite  →  alias  →  execute
```

Each stage either passes the request through unchanged or rewrites the
target `(provider, model_id)` before handing it to the next stage. All four
stages run inside the handler layer — the SDK `GproxyEngine::execute` step
only does the final upstream call.

## The unified `models` table

Since v1.0.5, real models and aliases live in the same `models` table. The
distinction is a single column:

- `alias_of = NULL` — a real model entry. It has a provider, a model id,
  and optionally pricing metadata.
- `alias_of = Some(id)` — an alias pointing at another row's id in the
  same table.

This table is what the admin API and the console read from and write to.
The engine rebuilds its in-memory alias lookup (`HashMap<String,
ModelAliasTarget>`) from this table at startup and on reload.

## Defining aliases

In the seed TOML:

```toml
[[model_aliases]]
alias = "chat-default"
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
enabled = true
```

At runtime, use the *Models* tab in the provider workspace of the embedded
console. Seed-time `[[model_aliases]]` rows are imported into the unified
`models` table on startup.

## Alias resolution at request time

When a client sends `"model": "chat-default"`, the pipeline:

1. **Permission check** — does the user have a permission row whose
   `model_pattern` matches the alias name?
2. **Rewrite rules** — channel-level rewrite rules can rewrite the alias to
   a different string before alias lookup.
3. **Alias resolution** — the alias is looked up in the unified `models`
   table and resolved to a concrete `(provider, model_id)` pair.
4. **Execute** — the engine prepares and issues the upstream request using
   the resolved pair.

Non-stream responses have their `"model"` field rewritten back to the alias
the client sent. Streaming chunks are rewritten per chunk in the engine.
From the client's perspective, the alias is a real model — it just happens
to price and route to something else.

## Pulling upstream models

The console's *Models* tab has a **Pull Models** button, which calls
`POST /admin/models/pull`. That endpoint fetches the upstream's real
`model_list` for a provider and returns the ids. The console imports them
into the local `models` table as real entries (`alias_of = NULL`) with no
pricing, which an admin can then edit.

This gives you the "seed from upstream, customize locally" workflow without
having to hand-edit TOML.

## `model_list` / `model_get` dispatch

How the model-list endpoints behave depends on the dispatch template
configured for the route:

- **`*-only` presets** (`chat-completions-only`, `response-only`,
  `claude-only`, `gemini-only`) default `model_list` and `model_get` to the
  **Local** implementation. Requests are answered entirely from the local
  `models` table and never hit upstream.
- **`*-like` / pass-through presets** still call upstream for `model_list`,
  but GPROXY **merges** the upstream response with the local `models`
  table: real local models that aren't in the upstream response are
  appended, and aliases mirror their target entry. `model_get` checks the
  local table first and falls through to upstream only on miss.

`GproxyEngine::is_local_dispatch(...)` lets handlers decide this before
calling `engine.execute`.

## Pricing and aliases

Billing tries to price a request against the **alias name** first and falls
back to the resolved real model name if no alias-level pricing exists.
This means admins can set different prices for the same real model under
different aliases — for example, to mark up a `premium-gpt4` alias while
keeping `chat-default` at cost.
