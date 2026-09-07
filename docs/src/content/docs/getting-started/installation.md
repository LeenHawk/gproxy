---
title: Installation
description: Install each GPROXY package, find the data directory and logs, complete first boot, update, manage login startup, and uninstall.
---

GPROXY is one executable, `gproxy`, with the console, portal, and public site
embedded. Every package on the [Downloads](/getting-started/downloads/) page
contains that executable and differs only in how it is started and where its
data lives.

## Packages

### Linux `.deb`

```bash
sudo apt install ./gproxy-linux-x86_64.deb
```

The package installs `/usr/bin/gproxy`, the launcher `/usr/bin/gproxy-desktop`,
an application-menu entry named GPROXY, and `/etc/xdg/autostart/gproxy.desktop`
so the launcher runs at login. It depends on `curl`, `xdg-utils`, and `zenity`.
The launcher creates `${XDG_DATA_HOME:-~/.local/share}/gproxy`, writes a
private `.env` there on first run, starts `gproxy` from that directory when
`http://127.0.0.1:8787/admin` is not answering, and opens the console. Running
`gproxy` yourself from a terminal uses `./data` in the current directory
instead — a separate instance with separate data.

### macOS `.dmg`

Drag `GPROXY.app` to Applications and open it. The app runs the server from
`~/Library/Application Support/GPROXY`, writes a private `.env` there on first
run, registers `~/Library/LaunchAgents/io.github.leenhawk.gproxy.plist` so the
server starts at login and is kept alive, and opens the console. It has no
Dock icon. The bundle is ad-hoc signed, not notarized, and requires macOS 11.

### Windows `.msi`

The installer is per-user and needs no administrator rights. It places
`gproxy.exe` and the launcher scripts in `%LOCALAPPDATA%\Programs\GPROXY`, a
Start menu shortcut that starts the server if needed and opens the console,
and a Startup-folder shortcut that starts it hidden at sign-in. The launcher
works in `%LOCALAPPDATA%\GPROXY` and writes a private `.env` there on first
run.

### Android `.apk`

The package id is `io.github.leenhawk.gproxy` and the minimum version is
Android 9 (API 28). Allow installation from unknown sources, install the APK
for your ABI, and open GPROXY. The screen has a switch "Start automatically on
app launch and device boot" (on by default), buttons to start the server, open
the console, and stop, and a log view. With automatic start on, the app asks
to be excluded from battery optimization. The service copies the binary into
the app's private storage, writes a private `.env`, listens on
`127.0.0.1:8787`, and shows a persistent notification while running.

### Portable Archive

```bash
unzip gproxy-linux-x86_64.zip -d gproxy && cd gproxy
chmod +x ./gproxy
./gproxy
```

Data goes to `./data` under the current directory unless `--data-dir` or
`GPROXY_DATA_DIR` says otherwise. On Android keep `gproxy`, `gproxy.bin`, and
`libc++_shared.so` together and run `./gproxy`.

### Container

```bash
docker run -d --name gproxy -p 8787:8787 \
  -v gproxy-data:/var/lib/gproxy \
  ghcr.io/leenhawk/gproxy:<tag>
```

The image sets `GPROXY_HOST=0.0.0.0`, `GPROXY_PORT=8787`,
`GPROXY_DATA_DIR=/var/lib/gproxy`, and `GPROXY_PERSISTENCE=sqlite`, runs as
the unprivileged user `gproxy`, and exposes port 8787. Mount
`/var/lib/gproxy`. See [Container](/deployment/docker/).

### Edge

Edge bundles are deployed with the platform's tooling and need a libSQL
database. See [Edge Wasm](/deployment/edge/).

## Where Data Lives

| Package | Data directory | Logs |
| --- | --- | --- |
| Portable | `./data` under the working directory | stdout and stderr |
| `.deb` launcher | `${XDG_DATA_HOME:-~/.local/share}/gproxy` | `${XDG_STATE_HOME:-~/.local/state}/gproxy/gproxy.log` |
| `.dmg` | `~/Library/Application Support/GPROXY` | `~/Library/Logs/GPROXY/gproxy.log` |
| `.msi` | `%LOCALAPPDATA%\GPROXY\data` | `%LOCALAPPDATA%\GPROXY\logs\gproxy.log` and `gproxy-error.log` |
| `.apk` | app-private storage, `files/data` | the log view in the app |
| Container | `/var/lib/gproxy` | container stdout |

The data directory holds `gproxy.db` (the SQLite store), the
`.autostart-initialized` marker, and `.update/` while an update is staged.
With the `libsql`, `postgres`, or `mysql` backends the directory still exists
for these files.

Configuration is layered: command line, then process environment, then
`./.env` in the working directory, then `<data-dir>/.env`, then defaults. Only
`GPROXY_*` keys plus `UPSTASH_URL` and `UPSTASH_TOKEN` are read from a `.env`;
other keys are ignored. The full list is `gproxy --help` and
[Configuration](/reference/configuration/).

:::caution[Keep the generated master key]
Each installer and the Android app write a random `GPROXY_MASTER_KEY` into
their private `.env` (the `.deb` and `.dmg` launchers also write
`GPROXY_DATA_DIR=.`). Stored credentials and user keys are then encrypted with
that key, and GPROXY refuses to open a sealed database without it. Back up the
`.env` together with `gproxy.db`. A portable install stores secrets in
plaintext until you set the variable yourself.
:::

## First Boot

Open `http://127.0.0.1:8787/admin`. While the store has no administrator the
console shows **Create the administrator**; choose a username and password and
you are signed in. The administrator is a user with the admin flag; the same
account signs in to the console, and its API keys are ordinary user keys.

For unattended setups the administrator can come from the environment:

```env
GPROXY_ADMIN_USER=admin
GPROXY_ADMIN_PASSWORD=<strong password>
# GPROXY_BOOTSTRAP_ADMIN_API_KEY=sk-<your-key>
# GPROXY_BOOTSTRAP_CHANNELS=openai,claudeapi
```

On a fresh store this creates the administrator, issues it an API key (the
one supplied, or a generated one you read from Identity with the reveal
action), and creates one enabled provider per listed channel id, named after
the channel, with the channel's routing defaults, an empty private rule set,
and no credentials yet. The
bootstrap key and channels require `GPROXY_ADMIN_PASSWORD` and are ignored
once an administrator exists. `GPROXY_ADMIN_PASSWORD` on an existing store
resets the password of the administrator named by `GPROXY_ADMIN_USER` if that
account exists and does nothing otherwise, so remove it after first boot
unless a reset is intended.

## Updating

Native installations update from **Updates** in the console. The update
channel (**Build default**, **Dev (alpha)**, **Stable releases**, **Staging
(prerelease)**) and the automatic-check switch (off by default) are instance
settings shared by every administrator. **Check for updates** fetches the
channel's manifest and shows the installed and latest versions, resolved
channel, build target, restart mode, and release notes.

**Verify and apply** downloads the manifest and refuses it unless its
Ed25519 signature verifies, its channel matches, it lists the running target
triple, and its minimum data version is not newer than this binary's schema.
It then downloads the archive, checks size and SHA-256, stages the executable
under `<data-dir>/.update/`, copies the running executable to `<exe>.prev`,
and swaps it. **Roll back** restores `<exe>.prev`. On Android the verified APK
is staged and the system package installer opens; the app needs permission
to install unknown apps.

| Variable | Effect |
| --- | --- |
| `GPROXY_UPDATE_CHANNEL` | Overrides the saved channel: `releases`, `staging`, or `dev`. |
| `GPROXY_UPDATE_CHANNEL_SERVE` | Same, and wins over `GPROXY_UPDATE_CHANNEL`. |
| `GPROXY_UPDATE_SERVE` | Manifest URL to fetch instead of GitHub, for a mirror. |
| `GPROXY_UPDATE_RESTART` | `re-exec` (default): re-execute the new binary in place after apply or rollback (exits 42 on Windows). `supervisor`: exit with code 42 so a process manager restarts it. `none`: restart the process yourself. |

The updater uses `GPROXY_UPSTREAM_PROXY_URL` when set. `releases` and `dev`
compare semantic versions; `staging` compares build hashes.

## Automatic Startup

**Settings → Automatic startup** manages a per-user autostart entry written by the
binary itself. On the first start in a data directory the entry is created
unless `GPROXY_AUTOSTART=off`; the decision is recorded in
`.autostart-initialized`. Linux needs a desktop session (`DISPLAY`,
`WAYLAND_DISPLAY`, or `XDG_CURRENT_DESKTOP`) and is skipped in containers.

| Platform | Entry |
| --- | --- |
| Linux | `~/.config/autostart/gproxy.desktop` (honours `XDG_CONFIG_HOME`) |
| macOS | `~/Library/LaunchAgents/io.github.leenhawk.gproxy.plist` |
| Windows | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `GPROXY` |

On Android, the APK home screen owns the startup switch and boot receiver.
In Termux, run `./gproxy` with the usual command-line flags and manage startup
from the Console. This writes `~/.termux/boot/gproxy.sh`; install Termux:Boot
and open it once to enable boot execution. The script preserves the working
directory and bundled library path, requests a wake lock, and writes output
to `autostart.log` in the data directory. Other Android shells have no automatic
startup integration. APK processes select APK updates; Termux processes select
the Android binary archive.

The entry records the executable, the flags it was started with, the working
directory, and `--master-key` copied from `GPROXY_MASTER_KEY` when that
variable was set, so treat it as secret-bearing. Turning the switch off removes
the entry and does not stop the running server. The `.deb` autostart file, the
`.msi` Startup shortcut, and the `.dmg` LaunchAgent belong to the installer's
launcher and are separate from this switch.

## Uninstalling

- `.deb`: `sudo apt remove gproxy`. The data directory, the log directory, and
  a per-user `~/.config/autostart/gproxy.desktop` remain.
- `.dmg`: delete `~/Library/LaunchAgents/io.github.leenhawk.gproxy.plist`,
  then move `GPROXY.app` to the Trash. The data and log directories remain.
- `.msi`: remove GPROXY from Windows Apps settings. `%LOCALAPPDATA%\GPROXY`
  and the `GPROXY` Run value, if enabled, remain.
- `.apk`: uninstall the app; Android removes its private storage with it.
- Container: `docker rm gproxy`; the volume persists until you remove it.
- Portable: delete the executable, `<exe>.prev`, and the data directory.

## Next Steps

- [Quick Start](/getting-started/quick-start/) to configure the first route.
- [Configuration](/reference/configuration/) for every flag and variable.
- [Console, Portal & Public Site](/guides/console/) before exposing the
  instance beyond localhost.
