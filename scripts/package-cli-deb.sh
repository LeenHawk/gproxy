#!/usr/bin/env bash
set -euo pipefail
: "${TARGET_TRIPLE:?}" "${TARGET_OS:?}" "${ARTIFACT_NAME:?}"
version="${GPROXY_BUILD_VERSION:-$(scripts/release-metadata.sh version)}"
output="${OUTPUT_DIR:-$PWD/dist/release}"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
root="$work/package"
mkdir -p "$root/DEBIAN"
dependencies=
case "$TARGET_OS" in
  linux)
    case "$TARGET_TRIPLE" in
      x86_64-*) architecture=amd64 ;;
      aarch64-*) architecture=arm64 ;;
      riscv64gc-*) architecture=riscv64 ;;
      *) exit 2 ;;
    esac
    prefix=/usr
    install -Dm755 "target/$TARGET_TRIPLE/release/gproxy" "$root$prefix/bin/gproxy"
    if [[ "$TARGET_TRIPLE" == *-gnu ]]; then dependencies='Depends: libc6, libgcc-s1, libstdc++6'; fi
    ;;
  android)
    case "$TARGET_TRIPLE" in
      x86_64-*) architecture=x86_64 ;;
      aarch64-*) architecture=aarch64 ;;
      *) exit 2 ;;
    esac
    prefix=/data/data/com.termux/files/usr
    source scripts/android/sdk.sh
    private="$prefix/lib/gproxy-cli"
    install -Dm755 "target/$TARGET_TRIPLE/release/gproxy" "$root$private/gproxy.bin"
    install -Dm644 "$(android_libcxx "$TARGET_TRIPLE")" "$root$private/libc++_shared.so"
    mkdir -p "$root$prefix/bin"
    cat > "$root$prefix/bin/gproxy" <<'LAUNCHER'
#!/data/data/com.termux/files/usr/bin/sh
set -eu
directory=/data/data/com.termux/files/usr/lib/gproxy-cli
export LD_LIBRARY_PATH="$directory${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$directory/gproxy.bin" "$@"
LAUNCHER
    chmod 755 "$root$prefix/bin/gproxy"
    ;;
  *) echo "unsupported DEB platform: $TARGET_OS" >&2; exit 2 ;;
esac
install -Dm644 LICENSE "$root$prefix/share/doc/gproxy-cli/copyright"
install -m644 README.md "$root$prefix/share/doc/gproxy-cli/README.md"
cat > "$root/DEBIAN/control" <<CONTROL
Package: gproxy-cli
Version: ${version/-/\~}
Architecture: $architecture
Maintainer: Leen Hawk <leenhawk@leenhawk.com>
Section: net
Priority: optional
Homepage: https://github.com/LeenHawk/gproxy
Description: GPROXY command-line server
 The CLI and server executable. The desktop application is a separate package.
CONTROL
if [ -n "$dependencies" ]; then printf '%s\n' "$dependencies" >> "$root/DEBIAN/control"; fi
dpkg-deb --root-owner-group --build "$root" "$output/$ARTIFACT_NAME.deb"
(cd "$output" && sha256sum "$ARTIFACT_NAME.deb" > "$ARTIFACT_NAME.deb.sha256")
