---
title: Development and Testing
description: Local development, tests, directories, and common commands.
---

## Common commands

Backend:

```bash
cargo fmt
cargo check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p gproxy
```

Frontend:

```bash
cd frontend/console
pnpm install
pnpm typecheck
pnpm build
```

## Provider regression scripts

Built-in scripts:

- `tests/provider/curl_provider.sh`
- `tests/provider/run_channel_regression.sh`

Examples:

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

## Contributing a new native channel

If custom channel mode is not enough for your upstream (for example custom auth flow, special path rules, custom response normalization, or deeper conversion behavior), contribute a native channel implementation.

### Backend integration checklist

1. Add a channel module under `sdk/gproxy-provider/src/channels/<your_channel>/`.
2. Implement needed files: `settings.rs`, `credential.rs`, `dispatch.rs`, `upstream.rs`, `mod.rs`.
3. Register channel enum and string id in `sdk/gproxy-provider/src/channel.rs`.
4. Register channel capability metadata and default dispatch in `sdk/gproxy-provider/src/registry.rs`.
5. Add settings parse/serialize wiring in `sdk/gproxy-provider/src/settings.rs`.
6. Wire channel execution in `sdk/gproxy-provider/src/provider.rs`.
7. If channel supports OAuth or upstream usage, wire corresponding runtime branches.

### Admin frontend integration checklist

1. Add channel files under `frontend/console/src/modules/admin/providers/channels/<your_channel>/`.
2. Register the channel in frontend channel registry so it appears in admin UI.

### Validation and regression

```bash
cargo check
cargo test --workspace
```

```bash
tests/provider/curl_provider.sh
tests/provider/run_channel_regression.sh
```

## Data directories

Default paths:

- data dir: `./data`
- default DB: `sqlite://./data/gproxy.db?mode=rwc`
- tokenizer cache: `./data/tokenizers`

`dsn` can be switched to mysql/postgres.
