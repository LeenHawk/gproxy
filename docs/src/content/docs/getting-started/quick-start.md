---
title: "Quick start"
description: "Add a provider and credential, create a model route, and send your first request."
---


Start GPROXY using the [installation guide](/getting-started/installation/). Application users complete the setup wizard and open the in-app console. CLI and container users open `http://127.0.0.1:8787/console/` and sign in with the administrator account created on first startup.

This example uses an OpenAI-compatible service. You need an upstream API key and a model ID supported by that service.

## 1. Add a provider

Open **Providers** and create a connection:

- Name it `openai-main`.
- For the official OpenAI service, select the `openai` channel.
- For another compatible service, select `custom`, enter its base URL, and select its supported protocols, such as Chat Completions (`openai_chat`). Check the upstream documentation for these values.

The provider name can also be used for direct requests. Multiple providers may use the same channel, with separate addresses and credentials.

## 2. Add and test a credential

Open the provider's **Credentials** tab and add the upstream API key. For channels such as Codex or Claude Code, choose the login option and follow its authorization steps.

Use the test action on the credential, select a supported model, and send a short message. Generation tests make a real upstream request and may incur charges. Save the working model ID for the next step.

The upstream key stays in GPROXY. Your client uses a gateway API key issued by GPROXY; these are different credentials.

## 3. Create a model route

Open **Model routes**, create a route named `fast`, and add a member. Select `openai-main` and enter the model ID you just tested. Keep the default tier and weight for now.

Clients can now request `fast`. You can change the upstream by editing the route member. Direct provider calls are also available without a route; see [Models and routes](/guides/models/).

## 4. Send a request

Use the gateway API key saved during setup, or create a new key under **My account → Keys**. Replace the placeholder below and adjust the port if needed.

```sh
export GPROXY_KEY='your-gproxy-api-key'
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"Hello. Please introduce yourself briefly."}]}'
```

An OpenAI client's Base URL setting usually takes `http://127.0.0.1:8787/v1`. A field asking for the complete request URL takes the `/v1/chat/completions` address above.

See [First request](/getting-started/first-request/) for other protocols and streaming, or [CLI clients](/guides/cli-clients/) for Codex CLI and Claude Code configuration.

## 5. Check usage

Open the console's request and usage pages to check the model, provider, token counts, and cost. Costs depend on configured pricing rules and are not a live copy of the upstream bill. Usage fields not reported by the upstream may be empty.

## Troubleshooting

| Result | Check first |
| --- | --- |
| Cannot connect | Whether GPROXY is running and the address and port are correct |
| `401` | Whether you supplied a valid GPROXY gateway key |
| `403` | The user or key's permission to access the target model |
| `404 unknown_model` | Whether `fast` exists, is enabled, and has a member |
| `429` | Local limits, upstream allowance, and credential availability |
| Upstream error or unsupported operation | Credential test results, model ID, protocol selection, and endpoint configuration |

For scripted management, see the API examples in [Providers](/guides/providers/) and [Models and routes](/guides/models/).
