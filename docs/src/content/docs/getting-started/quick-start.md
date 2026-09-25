---
title: Quick Start
description: "From a built binary to a metered request: start gproxy, add a provider and a credential, expose a model name, and call it."
---

This page takes a fresh instance to its first successful request. It assumes a
binary from [Installation](/getting-started/installation/) and uses the admin
API throughout, because a source checkout has no console bundle in it.

Every command below was run against a throwaway data directory; the output is
what came back.

## 1. Start the Instance

```sh
./target/release/gproxy serve --data-dir ./data --port 8787
```

The first start prints the administrator and one gateway API key, once, to
standard output:

```text
GPROXY first-run administrator (shown once)
  user:     admin
  password: zfpub7rfV2PO2PAbqazAVt-yC_yfwLrt
  api key:  sk-56sjXy3JADsZjYl1g3QnzzTNH-NBmzPNyUKf22qgVzI
Save these before closing this terminal; they are not stored in a form
this instance can show you again.
```

That key is both the gateway key and the administrator's key, so it opens
`/admin/api` as well as `/v1`. Keep it in a shell variable for the rest of this
page:

```sh
export GPROXY_KEY='sk-56sjXy3JADsZjYl1g3QnzzTNH-NBmzPNyUKf22qgVzI'
```

Check it is up. `/healthz` is unauthenticated and touches neither the database
nor the cache, so a load balancer polling it cannot become the load that fails
it:

```sh
curl -s http://127.0.0.1:8787/healthz
```

```json
{"revision":2,"status":"ok"}
```

## 2. See Which Channels This Binary Has

A channel is the adapter for one upstream family, and only the ones compiled in
exist. The list comes from the binary, not the database:

```sh
curl -s http://127.0.0.1:8787/admin/api/channels \
  -H "Authorization: Bearer $GPROXY_KEY"
```

A default build answers with all 27: `aistudio`, `antigravity`, `aws_bedrock`,
`azure`, `claudeapi`, `claudecode`, `claudeweb`, `cline`, `codex`,
`copilotcli`, `custom`, `dashscope`, `deepseek`, `devin`, `geminicli`,
`grokbuild`, `kimi`, `kiro`, `openai`, `opencodego`, `opencodezen`,
`openrouter`, `vercel`, `vertex`, `vertexexpress`, `workbuddy`, `xai`.

Each entry carries its login modes, its capabilities and the `config` keys a
provider form should offer — which is all a management UI needs to render one.

## 3. Add a Provider

A provider is one saved connection on a channel. `custom` is the generic
API-key channel: any endpoint that speaks OpenAI, Claude or Gemini natively.

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/providers \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{
    "name": "openai-main",
    "channel": "custom",
    "baseUrl": "https://api.openai.com",
    "config": { "dialects": ["openai_chat", "openai"] }
  }'
```

```json
{"id":"5a45fd807be02516a1626eedd528859d","name":"openai-main","channel":"custom",
 "baseUrl":"https://api.openai.com","connectionProfileId":null,
 "config":{"dialects":["openai_chat","openai"]},"enabled":true,
 "createdAtMs":1789981558211}
```

`name` is the operator's label and it is also the **provider mount**:
`/openai-main/v1/chat/completions` reaches this provider and nothing else.
`dialects` tells the `custom` channel which wire shapes this endpoint speaks —
name each one you want served; an operation whose dialect is not listed has no
conversion to reach it.

Keep the id:

```sh
export PROVIDER=5a45fd807be02516a1626eedd528859d
```

## 4. Add a Credential

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d "{\"providerId\":\"$PROVIDER\",\"label\":\"main key\",
       \"authKind\":\"api_key\",\"secret\":{\"api_key\":\"sk-…\"}}"
```

```json
{"id":"b8aad67f1af2cdb265218b8d1686a2ef","providerId":"5a45fd807be02516a1626eedd528859d",
 "organizationId":null,"teamId":null,"userId":null,"label":"main key",
 "authKind":"api_key","hasSecret":true,"version":0,"connectionProfileId":null,
 "metadata":{},"expiresAtMs":null,"status":"active","statusReason":null,"enabled":true}
```

The secret never comes back in a listing — only `hasSecret`, plus a separate,
audited `POST /admin/api/credentials/{id}/reveal`. Add as many credentials as
the account pool has; the engine rotates among them and fails over inside the
provider before the plan moves on.

For a channel that logs in rather than takes a key (`codex`, `claudecode`,
`kiro`, …), the credential comes from
[the login flows](/guides/providers/#acquiring-a-credential-by-logging-in)
instead.

## 5. Call It Through the Provider Mount

You can already send a request. The provider mount needs no route and no
exposed name:

```sh
curl -s http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"Say hello."}]}'
```

The `provider/model` form does the same thing from the aggregated mount:

```sh
curl -s http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"openai-main/gpt-4o-mini","messages":[{"role":"user","content":"Say hello."}]}'
```

## 6. Create a Model Route

A route name is the model name clients request. Its members choose the upstream
providers and models, with tiers and weights. Create `fast` and add a member:

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/routes \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"name":"fast"}'
```

```json
{"id":"33a88261f571347c7f0408c3bd2e2164","name":"fast","strategy":"round_robin",
 "maxAttempts":6,"enabled":true}
```

```sh
export ROUTE=33a88261f571347c7f0408c3bd2e2164

curl -s -X POST http://127.0.0.1:8787/admin/api/route-members \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d "{\"routeId\":\"$ROUTE\",\"providerId\":\"$PROVIDER\",
       \"upstreamModel\":\"gpt-4o-mini\"}"

```

```json
{"id":"fe91bb79f182e5becd899a056865c707","routeId":"33a88261f571347c7f0408c3bd2e2164",
 "providerId":"5a45fd807be02516a1626eedd528859d","upstreamModel":"gpt-4o-mini",
 "tier":0,"weight":100,"enabled":true}
```

A second member on another provider, at `tier: 1`, is a failover target. Two
members at `tier: 0` split traffic by weight.

Every write is one transaction with the configuration revision bump, and the
instance reloads before it tells its peers, so the new name works on the next
request.

## 7. Send the Request

```sh
curl -s http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"Say hello."}]}'
```

## 8. See What It Cost

```sh
curl -s http://127.0.0.1:8787/portal/api/usage \
  -H "Authorization: Bearer $GPROXY_KEY"
```

```json
{"fromMs":null,"toMs":null,"summary":{"requests":2,"inputTokens":4,
 "outputTokens":318,"cachedInputTokens":0,"cacheCreationTokens":0,
 "reasoningTokens":0,"cost":"0","currency":null,"truncated":false,"scanned":2},
 "groups":[],"trend":[]}
```

`cost` is `0` because no price rule covers this model yet. The request is still
settled and still recorded, with the dimension `unpriced = true` — the operator
wanted the signal that a model is being served for free, not a refusal. See
[Pricing & Tiers](/reference/pricing/).

## Next Steps

- [First Request](/getting-started/first-request/) — the same call in every
  accepted format, streaming, and the mounts.
- [Providers & Credentials](/guides/providers/) — pools, login flows, health.
- [Models, Routes & Exposed Names](/guides/models/) — tiers, weights and
  namespaces.
- [CLI Clients](/guides/cli-clients/) — pointing the Codex CLI and Claude Code
  at the gateway.
