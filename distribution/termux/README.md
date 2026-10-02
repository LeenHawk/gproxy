# Termux package submission

`gproxy/` is a source recipe for `termux/termux-packages`, pinned to the
v4.0.3 source archive. Copy this directory to `packages/gproxy/` in that
repository. It builds the embedded Console using the pnpm lockfile, then the
CLI using `Cargo.lock` and upstream's release profile (including fat LTO).
It does not download a prebuilt GPROXY executable or build the Tauri APK.

## Build

From a checkout of https://github.com/termux/termux-packages:

```sh
cp -R /path/to/gproxy/distribution/termux/gproxy packages/gproxy
./scripts/run-docker.sh ./scripts/lint-packages.sh packages/gproxy/build.sh
./scripts/run-docker.sh ./build-package.sh -a x86_64 -I gproxy
./scripts/run-docker.sh ./build-package.sh -a aarch64 -I gproxy
```

The output is in `output/`. Use the official builder image; building on the
phone is disabled because the BoringSSL dependency requires a host NDK.
ARM32 and i686 are excluded until supported and tested upstream.
Node, pnpm, Rust, CMake, Ninja and Go are build tools, not runtime dependencies.
The executable uses Termux's `libc++`, OpenSSL and CA certificates. SQLite
and the prefixed BoringSSL used for browser TLS emulation build from source.

Upstream release jobs use `bash scripts/build-termux.sh <target-triple>`.
This helper archives the checked-out **commit** (not uncommitted source),
copies the same recipe into the official builder, and substitutes that source
archive and the release version/channel/hash. `toolchain.json` pins the tested
builder image and packaging repository. It requires Docker with `/dev/fuse`
and mount permissions; `GPROXY_CONTAINER_ENGINE=podman` supports local Podman
validation. The source recipe retains upstream fat LTO. Release packaging
applies UPX `--best --lzma`, preserves the recipe's DEB control data, and creates
a ZIP containing the same executable and `TERMUX.txt`. Toolchain versions and
source checksum are recorded in internal provenance, not public attachments.

`package-manager-updates.patch` disables scheduled update checks and refuses
CLI/Console self-update and rollback. Package updates belong to APT. This
patch is confined to this package and does not change standalone builds.
The package conflicts with/replaces the upstream `gproxy-cli` DEB because both
own `$PREFIX/bin/gproxy`. User configuration and databases are not package files.

## Device validation

Install the DEB on the matching architecture inside Termux:

```sh
pkg update
pkg install curl
apt install ./gproxy_4.0.3_x86_64.deb # use the actual output filename
bash smoke-test.sh
```

`smoke-test.sh` uses a disposable data directory and loopback port 18787.
It checks the version, embedded Console, health endpoint, authenticated admin
API after restart, and refusal of self-update and rollback. It does not send
requests to a paid upstream. Provider forwarding requires a separate test
with a configured provider and credentials.

For upgrade testing, install the previous package first, then run
`bash smoke-test.sh /absolute/path/to/candidate.deb`. The script initializes
the old version, stops it, installs the candidate with APT, checks that the
version changed, and verifies the same admin key against the same database.
This also covers replacement of the upstream `gproxy-cli` DEB. Termux:Boot is
optional and requires its separate Android app; reboot/autostart testing must
be recorded separately.

## Submission boundary

Read the current [packaging policy](https://github.com/termux/termux-packages/blob/master/CONTRIBUTING.md#packaging-policy).
AGPL-3.0-or-later is an accepted license. Record the compressed package size
(policy limit: below 100 MiB), supported architectures, build logs, device
results and a reason for inclusion. The application includes a separately built
web Console and native dependencies; explain why the complete application is
not equivalent to installing only a Rust crate with Cargo. Acceptance also
depends on project activity/community and maintainer judgment. If the main
repository declines inclusion, consider https://github.com/termux-user-repository/tur.

Do not label a recipe as accepted, or Android CLI publication as migrated,
until the corresponding review/build/runtime evidence exists. Current local
validation results are recorded in `VALIDATION.md`.
