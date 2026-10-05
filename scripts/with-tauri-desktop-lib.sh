#!/usr/bin/env bash
# Android loads the cdylib; desktop binaries link the Rust library directly.
# Avoid a second full LTO link for a shared library that desktop packages omit.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$root/crates/gproxy-host-tauri/Cargo.toml"
backup="$(mktemp)"
cp -p "$manifest" "$backup"
trap 'cp -p "$backup" "$manifest"; rm -f "$backup"' EXIT
node - "$manifest" <<'JS'
const fs = require('node:fs');
const file = process.argv[2];
const text = fs.readFileSync(file, 'utf8');
const types = 'crate-type = ["lib", "cdylib"]';
if (!text.includes(types)) throw new Error('Unexpected Tauri library crate types');
fs.writeFileSync(file, text.replace(types, 'crate-type = ["lib"]'));
JS
python3 "$root/scripts/reproducible-run.py" "$@"
