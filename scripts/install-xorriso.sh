#!/usr/bin/env bash
set -euo pipefail
prefix="${RUNNER_TEMP:?}/gproxy-xorriso-1.5.6"
if [ ! -x "$prefix/bin/xorriso" ]; then
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  curl -fsSL --connect-timeout 15 --retry 2 https://ftpmirror.gnu.org/xorriso/xorriso-1.5.6.tar.gz -o "$work/source.tar.gz"
  printf '%s  %s\n' d4b6b66bd04c49c6b358ee66475d806d6f6d7486e801106a47d331df1f2f8feb "$work/source.tar.gz" | shasum -a 256 -c -
  tar -xzf "$work/source.tar.gz" -C "$work"
  (
    cd "$work/xorriso-1.5.6"
    ./configure --prefix="$prefix" --disable-libacl --disable-xattr
    make -j2
    make install
  )
fi
"$prefix/bin/xorriso" -version
printf '%s\n' "$prefix/bin" >> "${GITHUB_PATH:?}"
