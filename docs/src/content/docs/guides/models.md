---
title: "Models, Routes & Aliases"
description: "How a client model name resolves through aliases, variant suffixes, and routes to a provider credential, and how the model list is built"
---

A client model name is rarely an upstream model id. In aggregated mode the
request `model` resolves in a fixed order before a credential is chosen:

```text
request model
  -> alias (global, then provider-scoped when the provider is known)
  -> variant suffix (thinking level, service tier, ...)
  -> exposed model -> route -> members by tier and weight
  -> provider credential
```

In the console, a route is called a **load balancer**, an exposed model is a
**model mapping**, and an alias is a **routing alias**.

## Provider Models

Each provider keeps a catalogue of the upstream models it serves. A row has:

| Field | Meaning |
| --- | --- |
| Upstream model id | The id the provider expects. |
| Display name | Optional. |
| Max input, Max output | Context window and output limit, when known. |
| Thinking supported / adaptive / enabled | Capability flags. Unset means unknown. |
| Variants | Extra names that route to this model. See below. |
| Enabled | Disabled rows are never listed. |

**Pull from upstream** asks the provider for its live catalogue through the
ordinary list-models path, authenticated with your own key, and shows the
result. Nothing is written until you pick rows to import; known rows are
marked. When the embedded default catalogue knows a model, its limits fill
gaps and a default price rule can be created for this provider.

**Test** sends one 16-token chat completion for the model through the normal
pipeline with your own key. It passes admission, is billed, and reports the
status, latency, the key that paid, and the reply or the upstream error.

## Routes and Members

A route has a name, a maximum attempt count, and members:

| Field | Meaning |
| --- | --- |
| Provider, Upstream model | Where a member sends traffic. |
| Pinned credential | Optional. Restricts the member to one credential. |
| Failover tier | Default 0. Tier 0 is exhausted before tier 1 receives traffic. |
| Weight | Default 100. Splits traffic among healthy members in the same tier. |
| Enabled | Disabled members leave the plan. |

Members are ordered by tier, then health, then weight. One member of the
lowest healthy tier is chosen by a deterministic weighted counter, then a
credential inside it by the provider's strategy. Failover walks the rest of
the ordered list until the route's **Maximum attempts** is spent. Dead
credentials are excluded before the slot is consumed; degraded ones sort last.

## Exposed Models

A **model mapping** binds a public name to a route. What a route advertises is
folded from its members' provider-model rows, conservatively:

- a limit is known only when every member states one, and the minimum wins;
- a capability flag is `false` if any member says false, `true` only if all
  say true, otherwise unknown;
- a display name is kept only when all members agree;
- variants survive only when every member declares the same suffix.

A public name containing `/` creates a **namespace**: `team-a/reviewer` is
reachable as `reviewer` under `/team-a/v1/...`, and `GET /team-a/v1/models`
lists that namespace only.

## Aliases

An alias maps an incoming name to another name by exact match. Rows are
ordered by priority; the first enabled match wins.

| Scope | Applied |
| --- | --- |
| Any provider | Before route lookup, in every mode. |
| One provider | After the provider is known: named or scoped requests to that provider. |

Aliases are exact strings, not patterns. Use a variant when you want a family
of suffixed names.

## Variants and Suffix Presets

A provider model's **Variants** field declares extra names that route to the
base model. It is stored as a JSON array of names, or as an object when the
base name itself should not be listed:

```json
{ "expose_base": false, "variants": ["gpt-5-thinking-high", "gpt-5-tier-flex"] }
```

Variant names must be unique across the whole catalogue. The console's
**Set behavior** picker suggests suffixes per protocol and records what each
one injects:

| Protocol | Suffixes | Request field |
| --- | --- | --- |
| OpenAI Responses / Chat | `-thinking-none`, `-low`, `-medium`, `-high`, `-xhigh` | `reasoning.effort` / `reasoning_effort` |
| OpenAI Responses / Chat | `-tier-auto`, `-default`, `-flex`, `-scale`, `-priority`, `-fast` | `service_tier` (`-fast` = `priority`) |
| OpenAI Responses / Chat | `-effort-low`, `-medium`, `-high` | `text.verbosity` / `verbosity` |
| OpenAI Responses | `-image-generate`, `-image-edit`, `-search`, `-deep-research` | forced `tools` + `tool_choice` |
| Claude Messages | `-thinking-none`, `-low`, `-medium`, `-high`, `-adaptive` | `thinking` (budgets 1024 / 10240 / 32768) |
| Claude Messages | `-effort-low`, `-medium`, `-high`, `-xhigh`, `-max` | `output_config.effort` |
| Gemini | `-thinking-none`, `-low`, `-medium`, `-high` | `generationConfig.thinkingConfig.thinkingLevel` |
| OpenRouter, Vercel | `-via-<source>` | `provider.only` / `providerOptions.gateway.only` |

Thinking and tier suffixes are applied by the core itself: when the requested
name is a declared variant and stripping recognised suffixes yields the base,
the body's `model` is rewritten and the fields above are set for the target
protocol. Every other behaviour is stored as ordinary `rewrite` rules, filtered
by the variant name, in a rule set the console creates per provider (named
`<provider> · defaults`). You can inspect and edit them in
[Routing Rules & Rule Sets](/guides/rules/).

## Model Listing

`GET /v1/models` (and the Claude and Gemini list paths) is answered locally in
aggregated and namespace mode. The list is the union of:

1. exposed models and their variants, with the folded metadata;
2. the provider catalogues as `provider/model`;
3. a live refresh from every provider in the plan whose
   `auto_refresh_models` is on (the default), run concurrently.

Operator rows win over the wire: a row you disabled never appears, and a row
you recorded keeps your limits. The refresh never writes to the catalogue.
`GET /v1/models/{id}` looks the id up in the same list. Both operations pass
admission and record a zero-cost settlement. A named request such as
`GET /openai-main/v1/models` follows that provider's routing rule instead.

Permissions filter what a caller can see and call at the provider and
operation-group level; see
[Permissions, Rate Limits & Quotas](/guides/permissions/).
