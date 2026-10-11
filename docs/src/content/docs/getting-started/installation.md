---
title: "Installation"
description: "Choose a GPROXY 4.0 application, CLI, container, or hosted deployment."
---


Choose **Application** for a local graphical app, or **CLI** for a server. Both provide the same gateway features, with different startup and management interfaces.

Open the [download page](/download/) or the [latest stable release](https://github.com/LeenHawk/gproxy/releases/latest), then download the file for your OS and architecture. The rolling `nightly` release provides development snapshots.

## One-line install

The installer script installs the **CLI** or **headless CLI**. It detects the OS, architecture and libc, verifies the package against the release's `SHA256SUMS`, and asks before changing anything. It does not install the Application; choose it in the menu to be sent to the [download page](/download/) (on Windows, to Microsoft Store).

```sh
# Linux, macOS, Android (Termux)
curl -fsSL https://gproxy.leenhawk.com/install.sh | sh
```

```powershell
# Windows PowerShell
irm https://gproxy.leenhawk.com/install.ps1 | iex
```

The menus take English, Simplified Chinese or Traditional Chinese, and let you change the detected platform, the edition and the download source (GitHub, GitLab or CNB). You can then set up an instance:

- the instance directory and either a SQLite file or a PostgreSQL / MySQL DSN;
- the listen address and port;
- the administrator name and password (leave it empty to generate one, printed once);
- a global upstream proxy (`http://`, `https://`, `socks5://`, `socks5h://`);
- a background service (`gproxy service install`).

These settings are written to `.env` in the instance directory (mode 600) together with a generated `GPROXY_MASTER_KEY`. Back that file up. The administrator is created with `gproxy bootstrap admin`. The proxy is saved to the database as the global proxy setting, the same value as in Console → Settings. To start the server by hand, run `gproxy serve` from that directory. Running the script again upgrades the binary. It keeps the existing configuration unless you choose to reconfigure, and it always keeps the master key.

For scripts and CI, pass options and skip the prompts. The PowerShell parameters use the same names in PascalCase (`-Edition`, `-Dsn`, `-AdminPassword`, `-Proxy`, `-Service`, `-Yes`); `--host` is `-ListenHost` and `--dir` is `-InstallDir`:

```sh
curl -fsSL https://gproxy.leenhawk.com/install.sh | sh -s -- -y \
  --edition headless --mirror cnb \
  --dsn 'postgres://gproxy:secret@db:5432/gproxy' \
  --host 0.0.0.0 --port 8787 \
  --admin-user admin --proxy socks5h://127.0.0.1:1080 --service
```

```powershell
& ([scriptblock]::Create((irm https://gproxy.leenhawk.com/install.ps1))) -Edition headless -Yes
```

| Option | Default |
| --- | --- |
| `--edition cli\|headless\|app` | `cli` |
| `--target linux-aarch64-musl` … | detected |
| `--channel release\|beta\|dev` | `release` |
| `--mirror github\|gitlab\|cnb` | `cnb` for Simplified Chinese, otherwise `github` |
| `--dir` | `~/.local/bin` (`/usr/local/bin` as root), Windows `%LOCALAPPDATA%\Programs\gproxy` |
| `--data-dir` | `~/.local/share/gproxy`, Windows `%LOCALAPPDATA%\gproxy` |
| `--db-path` / `--dsn` | `<data-dir>/gproxy.db` |
| `--host` / `--port` | `127.0.0.1` / `8787` |
| `--admin-user` / `--admin-password` | `admin` / generated; `GPROXY_ADMIN_PASSWORD` is also read |
| `--proxy` | none |
| `--service`, `--no-configure`, `--lang`, `-y` | off |

On Termux the script installs the matching DEB with `apt`; on OpenHarmony it unpacks the CLI ZIP beside its bundled `libc++`.

## Choose a package

| Platform | CLI (`gproxy-*`) | Application (`gproxy-tauri-*`) |
| --- | --- | --- |
| Linux GNU: x86_64, aarch64, riscv64 | ZIP, DEB | ZIP, DEB |
| Linux musl: x86_64, aarch64, riscv64 | ZIP, DEB | — |
| Windows: x86_64, aarch64 | ZIP, MSIX | ZIP, MSIX |
| macOS: x86_64, aarch64 | ZIP, DMG | ZIP, DMG |
| Android: x86_64, aarch64 | ZIP, Termux DEB | APK |
| OpenHarmony / HarmonyOS NEXT | ARM64, x86_64 ZIP | ARM64 unsigned HAP |

Choose x86_64 for most Intel / AMD computers and aarch64 for Apple Silicon. Linux Application requires WebKitGTK 4.1.

Android CLI runs inside Termux. Install its DEB with `apt install ./gproxy-android-<architecture>.deb`.
For the ZIP, first run `pkg install libc++ openssl ca-certificates`, then extract
it under Termux's home directory and follow `TERMUX.txt`. Program self-update is
disabled; install newer DEBs through APT. Use `pkg upgrade gproxy` only after an
enabled repository provides the package. Keep the same data directory and master key.

On Windows (x64, ARM64), both editions are available from Microsoft Store:

| GPROXY Desktop | GPROXY CLI |
| :---: | :---: |
| <a href="https://apps.microsoft.com/detail/9P2FJRB9RS4Z?mode=direct"><img src="https://get.microsoft.com/images/en-us%20dark.svg" alt="Get it from Microsoft: GPROXY Desktop" height="48"></a> | <a href="https://apps.microsoft.com/detail/9NBMH3S5K0L9?mode=direct"><img src="https://get.microsoft.com/images/en-us%20dark.svg" alt="Get it from Microsoft: GPROXY CLI" height="48"></a> |

GPROXY Desktop is the Application edition. Store installations are signed by Microsoft and update through the Store. You can also install them with `winget install --id 9P2FJRB9RS4Z --source msstore` (Desktop) or `winget install --id 9NBMH3S5K0L9 --source msstore` (CLI).

MSIX release attachments are Store submission packages and may not yet carry Microsoft's signature; install from the Store or use a ZIP. macOS apps use ad-hoc signing and are not notarized. The HarmonyOS HAP is unsigned and requires your own signature before installation. It has been verified on an emulator; background execution depends on the device, OS version and granted permissions.

For CI runners, use the headless CLI: it excludes the Web console and retains proxy and management APIs. The Release workflow publishes Linux, Windows, macOS and Android (Termux) ZIP packages for x86_64 and aarch64; Linux also includes riscv64, with GNU and musl variants. See [headless builds](/deployment/release-build/#headless-cli-for-ci) for downloads and build commands.

## Application: graphical setup

Install or extract Application, then open GPROXY. A new instance shows a three-step setup wizard:

1. **Connection**: choose a listening address, port, data directory, and database. Defaults are suitable for a first local setup.
2. **Administrator account**: enter a username and a password of at least 8 characters. Supply an API key or leave it blank to generate one.
3. **Import**: optionally import an existing v4 configuration file, or finish without one.

Save the service URL and API key shown on completion, then open the console. Desktop builds offer launch-at-login and a system tray. Mobile builds use private app storage and show the options their platform supports. Android also offers notification and background permission controls.

The app window manages the instance over local IPC. Its HTTP listener serves gateway API clients; opening that port in a browser does not open the app's wizard or management console.

## CLI: start a server

Extract the CLI ZIP and open a terminal in its directory. On Linux / macOS:

```sh
chmod +x ./gproxy
./gproxy serve --data-dir ./data --port 8787
```

On Windows, use PowerShell:

```powershell
.\gproxy.exe serve --data-dir .\data --port 8787
```

A new instance prints a generated administrator password and API key **once**. Save them. On restart, an explicit `GPROXY_ADMIN_PASSWORD` updates a same-name user’s password, or recovers and renames administrator `0` if no name matches; otherwise accounts are left unchanged.

Open **http://127.0.0.1:8787/console/** and sign in. Personal and administrative pages share this console. The CLI does not use the Application setup wizard.

The default listener is local-only, and the SQLite database is `./data/gproxy.db`. To accept connections from other devices, adjust the listening address and configure network access and HTTPS. See [Configuration](/reference/configuration/) for environment variables, database options, and master-key handling.

Set `GPROXY_MASTER_KEY` before adding upstream credentials and keep the same saved value for subsequent starts. Without it, the CLI stores upstream secrets unencrypted and reports this in its startup log.

## Containers

The release pipeline publishes images to both `ghcr.io/leenhawk/gproxy` and Docker Hub at `leenhawk/gproxy`. Stable versions use `vX.Y.Z` tags, beta uses `staging`, and development snapshots use `nightly`. Append `-musl` for musl images. Choose a published v4 tag; old v3 image tags do not run v4.

```sh
export GPROXY_IMAGE='leenhawk/gproxy:<tag>'
# Set GPROXY_MASTER_KEY to your saved 32-byte key (64 hex digits or base64).
docker run -d --name gproxy --restart unless-stopped \
  -p 127.0.0.1:8787:8787 \
  -v gproxy-data:/app/data \
  -e GPROXY_MASTER_KEY \
  "$GPROXY_IMAGE"
docker logs gproxy
```

Save the credentials from the first startup log and open `/console/`. The image runs as `65532:65532`; a bind-mounted directory must be writable by that user. Keep the `/app/data` volume when replacing a container. The release workflow builds GNU and musl images for amd64, arm64, and riscv64.

## Hosted platforms

Cloudflare Workers, Netlify, and Vercel all have deployment templates that require no local Rust toolchain. Cloudflare supports D1 or libSQL/Turso; the other two use PostgreSQL. For WebSocket / Realtime, choose Cloudflare or Vercel (WebSocket Beta).

See [Hosted deployments](/deployment/edge/) for deploy buttons, database choices, and first login. The same page also covers release bundles and Wrangler for manual Workers deployment.

## Upgrade from v3

Back up the database, master key, and startup configuration. Stop v3, then start v4 with the same configuration. Supported v3 SQLite, PostgreSQL, MySQL, and D1 databases migrate automatically. Read [Migrating v3 to v4](/deployment/v3-to-v4/) for retained data and the migration report to check.

To compile your own build, see [Building from source](/deployment/release-build/). Once installed, continue to [Quick start](/getting-started/quick-start/).
