---
title: "Installation"
description: "Choose a GPROXY 4.0 application, CLI, container, or Cloudflare Workers deployment."
---


Choose **Application** for a local graphical app, or **CLI** for a server. Both provide the same gateway features, with different startup and management interfaces.

Select a version on [Releases](https://github.com/LeenHawk/gproxy/releases), then download the file for your OS and architecture. Version 4.0.0 is being prepared; until it is published, use `nightly` to try v4. Nightly builds change with development and are not stable releases.

## Choose a package

| Platform | CLI (`gproxy-*`) | Application (`gproxy-tauri-*`) |
| --- | --- | --- |
| Linux GNU: x86_64, aarch64, riscv64 | ZIP, DEB | ZIP, DEB |
| Linux musl: x86_64, aarch64, riscv64 | ZIP, DEB | — |
| Windows: x86_64, aarch64 | ZIP, MSIX | ZIP, MSIX |
| macOS: x86_64, aarch64 | ZIP, DMG | ZIP, DMG |
| Android: x86_64, aarch64 | ZIP, Termux DEB | APK |
| OpenHarmony / HarmonyOS NEXT | ARM64, x86_64 ZIP | Experimental ARM64 HAP |

Choose x86_64 for most Intel / AMD computers and aarch64 for Apple Silicon. Linux Application requires WebKitGTK 4.1.

The MSIX release attachments are unsigned Store submission packages, not signed installers. Use a ZIP or check the actual Microsoft Store listing for availability. macOS apps use ad-hoc signing and are not notarized. The experimental HarmonyOS HAP requires your own signature, has not been verified on a physical device, and does not provide a background service.

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

A new instance prints a generated administrator password and API key **once**. Save them. Restarting an existing instance does not reset its accounts.

Open **http://127.0.0.1:8787/console/** and sign in. Personal and administrative pages share this console. The CLI does not use the Application setup wizard.

The default listener is local-only, and the SQLite database is `./data/gproxy.db`. To accept connections from other devices, adjust the listening address and configure network access and HTTPS. See [Configuration](/reference/configuration/) for environment variables, database options, and master-key handling.

Set `GPROXY_MASTER_KEY` before adding upstream credentials and keep the same saved value for subsequent starts. Without it, the CLI stores upstream secrets unencrypted and reports this in its startup log.

## Containers

The image is `ghcr.io/leenhawk/gproxy`. Obtain the image tag or commit SHA from the selected release and replace the placeholder below. Old v3 image tags do not run v4.

```sh
export GPROXY_IMAGE='ghcr.io/leenhawk/gproxy:<tag-or-commit-sha>'
# Set GPROXY_MASTER_KEY to your saved 32-byte key (64 hex digits or base64).
docker run -d --name gproxy --restart unless-stopped \
  -p 127.0.0.1:8787:8787 \
  -v gproxy-data:/app/data \
  -e GPROXY_MASTER_KEY \
  "$GPROXY_IMAGE"
docker logs gproxy
```

Save the credentials from the first startup log and open `/console/`. The image runs as `65532:65532`; a bind-mounted directory must be writable by that user. Keep the `/app/data` volume when replacing a container. The release workflow builds GNU and musl images for amd64, arm64, and riscv64.

## Cloudflare Workers

Use `gproxy-edge-cloudflare.zip` and follow [Edge deployment](/deployment/edge/) to configure the database and secrets. Workers Assets serves the console. WebSocket / Realtime uses the same routes as native deployments.

## Upgrade from v3

Back up the database, master key, and startup configuration. Stop v3, then start v4 with the same configuration. Supported v3 SQLite, PostgreSQL, MySQL, and D1 databases migrate automatically. Read [Migrating v3 to v4](/deployment/v3-to-v4/) for retained data and the migration report to check.

To compile your own build, see [Building from source](/deployment/release-build/). Once installed, continue to [Quick start](/getting-started/quick-start/).
