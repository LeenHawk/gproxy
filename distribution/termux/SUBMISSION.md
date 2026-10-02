# Draft: new package: gproxy 4.0.3

GPROXY is an AGPL-3.0-or-later AI API gateway with provider routing,
credential management and an embedded web administration Console.

- Homepage/source: https://github.com/LeenHawk/gproxy
- Version: https://github.com/LeenHawk/gproxy/releases/tag/v4.0.3
- Recipe: copy `gproxy/` from this directory into `packages/gproxy/`.
- Architectures: aarch64 and x86_64; upstream does not currently support
  Android arm/i686 builds.
- Runtime dependencies: libc++, openssl, ca-certificates.
- Package name/command: `gproxy`; conflicts with/replaces upstream's sideloaded
  `gproxy-cli` package because both install the same command.

The recipe builds the Console with the checked-in pnpm lockfile and compiles
the CLI with `Cargo.lock`. No prebuilt GPROXY binary is downloaded. The complete
application requires a separate web build and native TLS dependencies; a bare
Cargo installation does not build the embedded Console. At the 2026-10-02
policy check, the crates.io API for `gproxy` returned HTTP 404.

OpenSSL and libc++ use Termux packages. SQLite and prefixed BoringSSL build
from source. The patch prevents the CLI and Console from overwriting files
owned by the package manager. It also disables scheduled self-update checks.
The recipe installs the application license and bundled tokenizer notices.

Both architectures passed official package builds and ELF symbol checks.
The ordinary recipe's DEBs measured 14,316,532 bytes (x86_64) and 13,927,348
bytes (aarch64), below the 100 MiB packaging-policy limit. Android 15 x86_64
Termux testing covered fresh startup, embedded Console, authenticated admin
requests, restart persistence, replacement of upstream's v4.0.2 package, and
refusal of CLI/Console self-updates. See `VALIDATION.md` for the exact evidence
and untested boundaries.

This is submission text only. No issue or pull request has been submitted.
Main-repository acceptance remains a Termux maintainer decision; TUR is an
alternative if the main repository declines the package.
