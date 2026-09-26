# gproxy-host-tauri

English | [简体中文](README.zh-CN.md)

The application host for GPROXY v4 — the desktop shell and, from the same
source, [the Android application](ANDROID.md). One process, one instance, two
front doors:

```
        the window                     Claude Code, the Codex CLI
            │                                      │
       Tauri IPC                              HTTP, 127.0.0.1
            │                                      │
     ipc::table (this crate)        gproxy-host-axum, data plane only
            └───────────────┬──────────────────────┘
                       one gproxy_app::App
                   one Gproxy · one snapshot · one cache
```

Almost everything is in the library. The `gproxy-desktop` binary opens a window
and calls `run()`; it parses nothing and decides nothing, which is why the test
suite can drive the whole arrangement on a machine with no display server.

```rust,ignore
use gproxy_host_tauri::{Desktop, secrets::Keychain};

let desktop = Desktop::start(data_dir, &Keychain).await?;
println!("data plane on {}", desktop.data_plane().base_url);
desktop.shutdown();
```

## The data plane does not go through IPC

Its clients are other programs, and they speak HTTP. They cannot speak Tauri
IPC and never will, so the desktop process runs the real host —
[`gproxy-host-axum`](../gproxy-host-axum) — on `127.0.0.1` for them, over the
same `App` the IPC commands use. This crate does not *implement* a data plane;
it hosts the one that already exists.

Two things about that socket:

**It still demands a key.** The IPC channel is a trust boundary; a loopback
socket is not. Every process on the machine can reach it, and an
unauthenticated data plane would let any of them spend the user's upstream
quota. Same authenticator, same admission, same `401`.

**It serves only the data plane.** `/admin/api` and `/portal/api` answer `404`
with a body saying where they went. A server needs those routes — a browser is
how an operator reaches a machine they are not sitting at — but a desktop shell
has a window, and a surface that is not needed should not be reachable. That
also keeps the gateway key a use-this-instance credential rather than an
administration one.

### The port

`8787` by default, the same as the server. **Fixed rather than random**: a client is configured with a base URL that
is typed once and kept, and a port that moved on every launch would mean
editing every client on every launch. Change it with `port` in `gproxy.toml`.

## Configuration

Two sources, not the command line's five. A window has no arguments, no shell
environment worth reading and no `.env`.

1. `gproxy.toml` in the data directory — the same `AppConfig` schema the server
   reads, so there is no second document to keep in step;
2. the desktop's defaults.

The first-run wizard chooses the listening IP and port (default
`127.0.0.1:8787`), data directory, SQLite file or PostgreSQL/MySQL URL,
administrator credentials, initial API key, launch-at-login and tray behavior.
It can import a format-5 configuration export, including a source master key
when needed. The local application window remains signed in.

Non-secret setup choices live in `desktop-setup.json` under the platform's
application-data directory. Instance settings live in the selected directory's
`gproxy.toml`; on Unix this file is owner-readable/writable only, because a
connection URL can contain database credentials. Passwords and import keys are
not written to the setup marker. Existing instances bypass the wizard and keep
their accounts. A failed import leaves setup pending so it can be retried.

The console is always the application window. Even when the listener accepts
network clients, HTTP management endpoints stay disabled.

| Path | What it is |
|---|---|
| `{data}/gproxy.db` | the instance |
| `{data}/gproxy.toml` | optional; anything `AppConfig` understands |
| `{data}/files/` | published bodies and downloaded vocabularies |
| `{data}/secrets.json` | which store each secret went to — no key material |
| `{data}/gateway-key` | `0600`, and only when there is no keychain |

## Secrets

| Secret | Keychain | No keychain |
|---|---|---|
| master key | 32 bytes, minted on first run | **no key at all**, and the warning `gproxy` already has |
| gateway key | the token bootstrap minted | a `0600` file beside the database |

The keychain is [`keyring`](https://docs.rs/keyring): Secret Service on Linux,
the Keychain on macOS, the Credential Manager on Windows.

The master key does **not** fall back to a file. A key sitting next to the
database it protects is not encryption, it is a longer path to the same
plaintext. When there is no keychain the instance runs exactly as the server
does without `GPROXY_MASTER_KEY` — and says so in the same words, because
`gproxy::rotate::PLAINTEXT_SECRETS` is one sentence both hosts use and only the
remedy differs. `desktop_instance_status` reports `secretsAreSealed: false` so
the window can say it where a person will see it.

The gateway key does fall back to a file. It is a different kind of secret: it
grants use of *this* instance rather than of somebody's upstream account, the
data plane cannot run without it, and the alternative is an application that
refuses to start on a machine with no keyring daemon.

**A keychain that has lost an entry it is supposed to hold is a refusal to
start.** `secrets.json` records which store the master key went into; without
it, a cleared login keyring would look like a first run, a *new* key would be
minted, and every credential already in the database would silently become
unopenable. The refusal names the entry to restore.

## The IPC command table

261 commands, generated from one declaration in
[`src/ipc/table.rs`](src/ipc/table.rs):

```
admin     91      gproxy_app::Operations — the identity families
manage   137      gproxy.manage()       — the engine's configuration
portal    14      gproxy_app::Portal    — the end user's own surface
query     11      gproxy.query()        — usage, quota, logs
upstream   5      gproxy.login()        — the three upstream login flows
desktop    3      the shell's own state
```

A command name is `{surface}_{family}_{method}` — `admin_users_list` is
`Operations::users().list(..)`. The table is keyed on **operations, never on
the HTTP host's routes**: the two surfaces are siblings over one product layer,
so a router reorganisation in `gproxy-host-axum` is not a change here.

```
admin users [.users()] {
    list(query: app::ListQuery);
    get[id];
    create(write: app::UserWrite);
    update[id](patch: app::UserPatch);
    delete[id];
    batch(items: Vec<app::BatchItem<app::UserWrite, app::UserPatch>>);
    set_password[id, new_password];
}
```

Bracketed names become `String` parameters passed as `&str`; parenthesised ones
are deserialized and passed by value. An `@manual { … }` block at the end names
the sixteen methods whose shape the grammar deliberately does not cover — a
slice argument, a clock the webview must not supply, a synchronous accessor
that answers no `Result` — and they are ordinary functions listed in the same
inventory.

A command does exactly three things: unpack, call one method, render. There is
no validation, no authorization and no branching on the answer, because every
rule already has an owner below this line and a second opinion here would apply
over IPC and not over HTTP.

To read the table without expanding a macro:

```sh
cargo test -p gproxy-host-tauri --test table -- --ignored --nocapture
```

### Authentication over IPC

There is none, deliberately. A message arrives only because this process's own
webview sent it, so the channel is the proof; a key check would be the
application authenticating itself to itself. `Desktop::caller` is the local
administrator, and the `portal_*` commands are scoped to them by construction
exactly as they are for a signed-in browser.

The first-run wizard sets the administrator password through the existing
bootstrap and password-hashing operations. It does not add a lock screen to the
local window. Existing accounts are preserved when opening an existing database.

### How the console reaches the instance

The window runs the same console the server serves. `pnpm build` in `console/`
writes it to `ui/console/` and its page to `ui/index.html` (the window opens `/`,
and Tauri answers unknown paths with that page). `ui/` is a build artefact and
is not committed; CI downloads the console build into it.

The console makes every call through one `fetch` of an `/admin/api` or
`/portal/api` path. In the window, `console/src/lib/transport.ts` sends that
request as `desktop_console_request`, and `src/console.rs` passes it, in
process, to `gproxy_host_axum::router`: the same routes, admin scopes, section
gates, audit and refresh the server has. The response comes back over IPC.
Nothing new listens on a socket: the embedded data plane still refuses both
surfaces. The request is authenticated with the gateway key, which belongs to
the local administrator. Cookies and `Authorization` headers from the page are
dropped.

The operation table in `src/ipc/table.rs` stays for callers that want an
operation by name instead of a path.

## Desktop integration

Launch-at-login uses a per-user desktop entry on Linux, a LaunchAgent on macOS,
and a Windows Run entry or MSIX StartupTask. With the tray enabled, closing the
window keeps the gateway running; the tray can reopen the window or quit the app.
Launching the application again brings the existing window forward.

Desktop automatic updates are not implemented. Android uses its foreground
service, boot receiver and APK updater; see [ANDROID.md](ANDROID.md).

It also does not build the console, and it does not bind the OAuth issuer:
`/v1/oauth/*` is the authorization server this instance runs for *downstream*
clients, reached over HTTP by programs that redirect a browser at it. Invoking
`token` over the IPC bridge of the application issuing it has no meaning.

## Reused rather than reimplemented

[`gproxy`](../gproxy) — the command line's library half — already turns an
`AppConfig` into a running instance (`instance::open`), rotates the master key,
creates the first administrator (`bootstrap::ensure_admin`) and installs the
log subscriber. All four are called here. What does not carry over is the
*configuration layering*: `clap` over the environment over `.env` is the
operator's interface, and a window has none of the three.

## Building

Tauri's Linux dependencies are `webkit2gtk-4.1`, `gtk+-3.0` and `libsoup-3.0`.
Because of them this crate is **not in the workspace's `default-members`**: a
bare `cargo check` skips it, and `cargo check --workspace` builds it. See the
comment in the root `Cargo.toml` for the measurement behind that.

```sh
cargo check   -p gproxy-host-tauri
cargo clippy  -p gproxy-host-tauri --all-targets --all-features -- -D warnings
cargo test    -p gproxy-host-tauri
cargo run     -p gproxy-host-tauri --bin gproxy-desktop
```

Android needs none of those libraries and all of the Android SDK instead:

```sh
cd crates/gproxy-host-tauri
pnpm install                 # the Tauri CLI, pinned
pnpm android:build:arm64     # a release APK
```

The library carries a `cdylib` crate type for it — there is no binary on a
phone, the activity loads `libgproxy_host_tauri.so` — and `src/main.rs` is not
built for Android at all. [ANDROID.md](ANDROID.md) has the toolchain versions,
what a CI job would need, and what is unverified.

The tests never touch a real credential store: `secrets::MemoryStore` and
`secrets::UnavailableStore` stand in for a working keychain and a missing one,
and `tests/assembly.rs` starts a whole instance against a temporary directory
and dispatches through the real command table over Tauri's `MockRuntime`.
