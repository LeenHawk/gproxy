#!/usr/bin/env bash
# btls-sys regenerates and audits BoringSSL's symbol prefixes with Go.
set -euo pipefail
apt-get update
apt-get install -y --no-install-recommends clang
archive=go1.27.1.linux-amd64.tar.gz
checksum=63d339f0da5ab53635a56f2490a7984dfe12dfcff22ad749f63edaf590168445
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
curl --fail --location --retry 3 "https://go.dev/dl/$archive" -o "$work/$archive"
printf '%s  %s\n' "$checksum" "$work/$archive" | sha256sum --check
tar -C /usr/local -xzf "$work/$archive"
ln -sf /usr/local/go/bin/go /usr/local/bin/go
go version
