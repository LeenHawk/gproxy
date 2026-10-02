# Temporary glib security backport

This directory contains the crates.io glib 0.18.5 source, with the two-line
VariantStrIter fix from https://github.com/gtk-rs/gtk-rs-core/pull/1343
(commit 05dff0ee696f9bcd8617cd48c4b812d046d440cb) and an explanatory comment.
The trailing blank line in LICENSE is removed for git whitespace checks.
It fixes GHSA-wrw7-89jp-8q8g / RUSTSEC-2024-0429 (gproxy Dependabot #81):
the variadic C out-argument must receive a mutable pointer reference.

Tauri's GTK3 dependency chain still uses glib 0.18; glib 0.20 is not a
drop-in replacement. The root Cargo.toml patches crates.io to this copy.
The original version is retained to preserve dependency compatibility.

Once the Tauri/GTK dependency chain uses an upstream glib containing this
fix (0.20+ or a compatible backport), remove this directory, the root
glib patch and workspace exclusion, then regenerate Cargo.lock.

Run the existing iterator tests with optimizations, where the original
undefined behavior can cause a null-pointer crash:

    cargo test --manifest-path vendor/glib/Cargo.toml --lib --release variant_iter --target-dir target/glib-backport
