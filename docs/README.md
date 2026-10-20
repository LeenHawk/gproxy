# Documentation build

Use the Rust and Node versions pinned in `.github/workflows/ci.yml`, then:

```sh
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs build
```

Both `build` and `dev` first run `generate:reference`. That command runs the
`gproxy-host-axum` `export-docs` example with the optional `documentation`
feature and all channels named by the SDK's existing `channels` feature group.
It does not start the gateway, open a database or contact upstream providers.
After editing Rust while `astro dev` is running, regenerate with:

```sh
pnpm --dir docs generate:reference
```

The Rust exporter writes ignored build inputs to `public/generated/`:

- `openapi.json`: `utoipa` describes the HTTP operations; `schemars` derives
  complete request/response schemas from wire types and management DTOs.
  Forwarding, management, portal, OAuth and system APIs share one specification.
  SSE event schemas use `x-event-schema` beside the text stream; WebSocket
  messages use `x-websocket`. Multipart files and binary responses retain their
  content types. Input/output contracts have separate component namespaces.
  The build parses actual management/portal router declarations with `syn` and
  refuses to export if any method/path is missing or added only in the docs.
- `configuration.schema.json`: `schemars` derives the shared `AppConfig`
  shape, Serde names, defaults and documentation from Rust types. Runtime
  validation such as master-key decoding is not represented by this schema.
- `configuration.defaults.json`: `AppConfig::default()`, not local settings.
- `channels.json`: compiled channel descriptors, host configuration keys and
  vendor service classifications. Declarations do not prove live account
  access or support for every protocol operation. Placeholders are not defaults.

The English, Simplified Chinese, Traditional Chinese and Russian MDX pages render these
JSON files through Astro components. `src/i18n/reference/` translates all
generated descriptions; untranslated additions fail the build. Protocol keys,
enum values, source paths and type identifiers remain unchanged. The build also
writes `*.zh-cn.json`, `*.zh-tw.json` and `*.ru.json` with localized descriptions.

The API reference links every payload to its complete data-structure definition.
`starlight-llms-txt` renders those same pages into `llms-full.txt`,
`llms-small.txt` and `_llms-txt/api-and-configuration.txt`. `llms.txt` links
the documentation sets and machine-readable exports. The JSON files are also
published as static files; generated outputs are never committed.

`/llms-api.txt`, `/zh-cn/llms-api.txt`, `/zh-tw/llms-api.txt` and `/ru/llms-api.txt` contain the
complete OpenAPI schemas, configuration schema and channel catalogue in each
language. Unlike the compact overview, these files never remove expandable
payload definitions. `llms.txt` links all four language variants.

Keep each localized content tree aligned with the English pages, including
sidebar labels, download-page copy and localized internal links. Russian
tutorial pages reuse the English video with `ru.vtt` subtitles; they do not
claim a Russian audio track. The privacy page is generated from
`distribution/mobile/privacy/privacy-ru.txt` by `scripts/mobile/sync-listings.py`.

Normal gateway builds do not enable the optional `schema` or `documentation`
features. No runtime documentation endpoint is added. Required-but-nullable
wire fields carry `x-gproxy-required` during derivation; the exporter converts
it into JSON Schema `required` without stripping null from the accepted values.
