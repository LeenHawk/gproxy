# GPROXY

[English](README.md) · [简体中文](README.zh-CN.md) · [Русский](README.ru.md)

[![Release](https://img.shields.io/github/v/release/LeenHawk/gproxy)](https://github.com/LeenHawk/gproxy/releases/latest)
[![CI](https://github.com/LeenHawk/gproxy/actions/workflows/ci.yml/badge.svg)](https://github.com/LeenHawk/gproxy/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue)](LICENSE)
[![FOSSA Status](https://app.fossa.com/api/projects/custom%2B37316%2Fgithub.com%2FLeenHawk%2Fgproxy.svg?type=shield)](https://app.fossa.com/projects/custom%2B37316%2Fgithub.com%2FLeenHawk%2Fgproxy?ref=badge_shield)

[Documentation](https://gproxy.leenhawk.com/) · [Downloads](https://github.com/LeenHawk/gproxy/releases) · [Discussions](https://github.com/LeenHawk/gproxy/discussions) · [Sponsor](https://github.com/sponsors/LeenHawk)

<a href="https://deploy.workers.cloudflare.com/?url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fcloudflare-button"><img src="https://deploy.workers.cloudflare.com/button" alt="Deploy to Cloudflare" height="32"></a>
<a href="https://app.netlify.com/start/deploy?repository=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&branch=dev&create_from_path=deploy%2Fserverless"><img src="https://www.netlify.com/img/deploy/button.svg" alt="Deploy to Netlify" height="32"></a>
<a href="https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fserverless&project-name=gproxy&repository-name=gproxy&env=GPROXY_ADMIN_PASSWORD%2CGPROXY_MASTER_KEY&products=%5B%7B%22type%22%3A%22integration%22%2C%22group%22%3A%22postgres%22%2C%22protocol%22%3A%22storage%22%7D%5D"><img src="https://vercel.com/button" alt="Deploy with Vercel" height="32"></a>
<a href="https://render.com/deploy?repo=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev"><img src="https://render.com/images/deploy-to-render-button.svg" alt="Deploy to Render" height="32"></a>
<a href="https://heroku.com/deploy?template=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev"><img src="https://www.herokucdn.com/deploy/button.svg" alt="Deploy to Heroku" height="32"></a>

GPROXY is a self-hosted gateway for LLM APIs, written in Rust. You hand it
your upstream accounts: API keys, the OAuth logins behind subscriptions such
as Claude Code or Codex, or browser cookies. The people who use it get one
base URL, one GPROXY API key and model names that you choose. They never see
the upstream secrets.

GPROXY does not run models. Inference is done by the upstream service.

The full manual is at **<https://gproxy.leenhawk.com/>**.

![The console's provider page](.github/readme/providers-en.webp)

## When it is useful

- You have a few Claude or ChatGPT subscriptions and a pile of API keys, and
  you want to share them with a team or with friends. Everyone gets a key of
  their own with its own permissions, rate limits and budget, and you can see
  afterwards who used what.
- The same model is available from several sources and you want them to back
  each other up. When an account is rate-limited or fails, the request moves
  to the next one and the client does not need to know.
- The client only speaks the OpenAI format and the upstream is Claude or
  Gemini, or the other way round.
- The gateway has to stay up on a server, a NAS, in a container or on
  Cloudflare Workers.

If all you want is to switch models for Claude Code or Codex on your own
computer, a desktop switcher is less work. GPROXY does not edit those tools'
configuration files; you enter the gateway URL and key yourself.

## Features

### Subscription accounts as upstreams

Besides API keys, GPROXY can use subscription accounts as upstreams: Claude
Code, Claude on the web, Codex (a ChatGPT account), Gemini CLI, Antigravity,
GitHub Copilot CLI, Kiro, Grok Build, Kimi Code, Trae, Devin, WorkBuddy and
others. You sign in once in the console and GPROXY refreshes the tokens from
then on.

A provider can hold several accounts. You can look up each account's quota
windows, and an account that the upstream has rate-limited gives way to the
others. With session affinity turned on, a conversation stays on one account,
so the prompt cache is not lost by switching accounts.

Claude Code and Codex CLI send more than model requests. They also ask for
account details, usage, plugins and so on. GPROXY answers these too, so the
official CLIs work when pointed at the gateway. What the CLI shows is the
caller's own usage on the gateway; nothing about the shared account is
disclosed to them.

### Several users

Accounts are organized into users, organizations and teams, and an
administrator can be limited to one organization or team. Each key can be
restricted to certain providers and models and given rate limits and a
spending budget. A credential can belong to a user, an organization or a team,
and nobody else can use it.

Tools that sign in with OAuth, such as Claude Code and Codex CLI, can sign in
to your GPROXY instance directly, so you do not have to hand out a key to each
person.

Every upstream call is recorded with its token counts and cost. There are
also request logs and an audit log.

### Protocol conversion

Clients can use OpenAI Chat Completions, OpenAI Responses, Claude Messages or
Gemini GenerateContent, streaming or not. When the client and the upstream use
different formats, the gateway converts between them. Embeddings, rerank,
images, audio, video, files and Realtime are available where the upstream
offers them.

Conversion changes only the format of a request. A model that cannot call
tools or read images still cannot when it is asked in another format.

### Model routes

Clients ask for a name such as `fast` or `coding`, and you decide which
providers and upstream models serve it. Route members can be put in tiers; a
later tier is used only when nothing in the earlier ones is available. Within
a tier, requests are spread round-robin, by weight, or in a fixed order. You
can also do without routes and ask for `provider/model` directly.

![The console's model route page](.github/readme/routes-en.webp)

### Rewrite rules

Requests and responses can be rewritten per provider: system prompts, JSON
fields, regular-expression replacements, headers and prompt-cache
breakpoints. Some clients cannot set cache fields. For those, you can put an
agreed string in the prompt, and when the provider has the option turned on,
GPROXY replaces it with a cache breakpoint.

### Channels

Apart from the subscription channels above, the built-in channels include
OpenAI, Claude API, Google AI Studio, Vertex AI, AWS Bedrock, Azure,
OpenRouter, DeepSeek, GLM, Kimi, MiniMax, DashScope, Volcengine, xAI, NVIDIA
NIM and others; the [provider
guide](https://gproxy.leenhawk.com/guides/providers) has the full list. A
compatible service that differs only in its address and headers can be added
with the `custom` channel. When a service needs an adapter of its own, you can
write a channel plugin outside this repository, without forking, as described
in [Adding a channel](https://gproxy.leenhawk.com/development/adding-a-channel).

### Ways to run it

- A native server: the CLI for Linux, macOS and Windows with the web console
  built in, or a headless build without the console.
- A container.
- A desktop and mobile app with the console and a setup wizard built in.
- A Cloudflare Worker or a Netlify or Vercel function, with the same channels
  as the native build.
- A Rust library: embed it in your own program through `gproxy-sdk`.

The database can be SQLite, PostgreSQL or MySQL, or D1 or libSQL/Turso on
Cloudflare.

## Installing

On Linux and macOS:

```sh
curl -fsSL https://gproxy.leenhawk.com/install.sh | sh
```

On Windows (PowerShell):

```powershell
irm https://gproxy.leenhawk.com/install.ps1 | iex
```

The script downloads the latest stable CLI, asks a few questions (data
directory, port, administrator password) and can register a background
service. Run it with `--help` to see the options, for example `--edition
headless` for a build without the web console.

You can also download a package by hand from
[Releases](https://github.com/LeenHawk/gproxy/releases/latest). Each release
contains:

- `gproxy-*`: the CLI with the web console built in;
- `gproxy-headless-*`: the CLI without the console, for CI and small servers;
- `gproxy-tauri-*`: the desktop and mobile application.

GPROXY Desktop, CLI and Headless are also on the Microsoft Store
([Desktop](https://apps.microsoft.com/detail/9P2FJRB9RS4Z),
[CLI](https://apps.microsoft.com/detail/9NBMH3S5K0L9),
[Headless](https://apps.microsoft.com/detail/9ND7FQQMV566)), and the Android
app is on [F-Droid](https://f-droid.org/packages/com.leenhawk.gproxy.desktop/).
macOS builds are not notarized. The [download
page](https://gproxy.leenhawk.com/download) lists every platform and its
requirements.

Stable releases are tagged `vX.Y.Z`. The `staging` and `nightly` releases are
the beta and development channels.

## Getting started

Start the server with a data directory:

```sh
./gproxy serve --data-dir ./data --port 8787
```

The first time it runs in a terminal it asks a few setup questions. Without a
terminal it creates an administrator account and prints the password and a
gateway API key; save both. By default it listens on 127.0.0.1 and keeps its
SQLite database in `./data/gproxy.db`.

Set `GPROXY_MASTER_KEY` before you add any upstream credentials. Without it,
upstream secrets are stored unencrypted. Keep the key and the data directory
together when you move or upgrade the instance; see
[Configuration](https://gproxy.leenhawk.com/reference/configuration).

Then open <http://127.0.0.1:8787/console/> and:

1. Under **Providers**, add a provider (for example `openai-main` on the
   `openai` channel) and its API key, and test the credential.
2. Under **Model routes**, create a route named `fast` with that provider and
   the model you tested as its member.

Now any OpenAI-compatible client can use it with the gateway API key:

```sh
export GPROXY_KEY='your-gproxy-api-key'
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"Hello"}]}'
```

The same route answers Claude (`/v1/messages`) and Gemini
(`/v1beta/models/fast:generateContent`) requests. The
[tutorial](https://gproxy.leenhawk.com/getting-started/quick-start) goes
through this step by step, and [CLI
clients](https://gproxy.leenhawk.com/guides/cli-clients) shows how to point
Codex CLI and Claude Code at the gateway.

The desktop and mobile app runs the same steps as a setup wizard and has the
console built in.

## Hosted deployment

The deploy buttons at the top of this page use the templates described here. The
Cloudflare, Netlify and Vercel ones deploy a prebuilt bundle of the latest
stable release; nothing is compiled. Cloudflare uses D1 or
libSQL/Turso, the other two use PostgreSQL. Render and Heroku run the native
binary in a small container with PostgreSQL.

You will be asked for an administrator password and a master key. Read
[Hosted deployments](https://gproxy.leenhawk.com/deployment/edge) and
[Docker, Render and Heroku](https://gproxy.leenhawk.com/deployment/containers)
first; they cover database choice, plan costs and the limits of each platform
(Netlify has no WebSocket support, for example).

## Upgrading from v3

Stop v3 and back up its database, master key and startup configuration. Then
start v4 with the same configuration. SQLite, PostgreSQL, MySQL and D1
databases are migrated on first start; accounts, passwords, API keys and
usage history are kept, and anything that could not be mapped is listed in the
migration report. Do not run v3 and v4 against the same database at the same
time. Details are in [Migrating v3 to
v4](https://gproxy.leenhawk.com/deployment/v3-to-v4).

## Performance

Measured with the [v4.0.0 Linux x86_64
release](https://github.com/LeenHawk/gproxy/releases/tag/v4.0.0) on a Ryzen 7
8745H (8 cores, 16 threads), SQLite on NVMe, with the load generator
(`oha 1.16.0`), the gateway and a mock upstream on the same machine. Requests
were non-streaming Chat Completions passing through authentication, routing,
usage extraction, pricing and persistence. The mock returned 25 input and 18
output tokens with no delay. Body logging was off; usage recording and cost
settlement were on. Each figure is the median of three 10-second runs after a
2-second warm-up.

| Request logs | Connections | Requests/s | p50 | p99 |
| --- | ---: | ---: | ---: | ---: |
| Off | 1 | 2,882 | 0.34 ms | 0.50 ms |
| Off | 32 | 28,303 | 0.76 ms | 2.71 ms |
| Off | 64 | 27,263 | 1.47 ms | 41.91 ms |
| Upstream and downstream metadata | 64 | 7,765 | 2.81 ms | 45.16 ms |

With one connection the gateway adds about 0.30 ms to the median latency of
calling the mock directly. All 1,989,395 requests returned 200, and the stored
usage rows matched the request count with correct tokens and cost. Throughput
stops growing at around 64 connections, where p99 rises to about 42 ms. These
numbers are for short responses, same-protocol forwarding and local SQLite;
measure your own workload if it involves long streams, protocol conversion, a
remote database or real upstreams.

## Building from source

You need stable Rust, Go and Clang, and for the console Node.js 22.12 or later
and pnpm. The console has to be built before the binary, otherwise the binary
has no console to serve:

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build
cargo run -p gproxy -- serve
```

[Building from source](https://gproxy.leenhawk.com/development/building)
covers the headless build, the desktop app, the Workers bundle and the checks
CI runs. Contributors should also read
[CONTRIBUTING.md](CONTRIBUTING.md) and
[Architecture](https://gproxy.leenhawk.com/development/architecture); to add an
upstream, see [Adding a
channel](https://gproxy.leenhawk.com/development/adding-a-channel).

## Reporting bugs

Use [Issues](https://github.com/LeenHawk/gproxy/issues) for bugs and
[Discussions](https://github.com/LeenHawk/gproxy/discussions) for questions.
For anything you would rather not post publicly, email
[leenhawk@leenhawk.com](mailto:leenhawk@leenhawk.com). Report security
problems privately as described in [SECURITY.md](SECURITY.md).

## License

GPROXY is licensed under the GNU Affero General Public License, version 3 or
later; see [LICENSE](LICENSE). The library crates `gproxy-protocol`,
`gproxy-protocol-macros`, `gproxy-client`, `gproxy-client-sdk`,
`gproxy-cache`, `gproxy-file`, `gproxy-seaorm`, `gproxy-store` and
`gproxy-tokenizer` are MIT-licensed; each has its own license file.

## Powered by

[![JetBrains logo](https://resources.jetbrains.com/storage/products/company/brand/logos/jetbrains.svg)](https://jb.gg/OpenSource)

[![Deploys by Netlify](https://www.netlify.com/assets/badges/netlify-badge-color-accent.svg)](https://www.netlify.com)

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=LeenHawk/gproxy&type=Date)](https://www.star-history.com/#LeenHawk/gproxy&Date)
