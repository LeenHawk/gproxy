#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/reproducible-env.sh"
: "${TARGET_TRIPLE:?}" "${TARGET_OS:?}" "${ARTIFACT_NAME:?}"
version="${GPROXY_BUILD_VERSION:-$(scripts/release-metadata.sh version)}"
output="${OUTPUT_DIR:-$PWD/dist/release}"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
root="$work/package"
# Keep the official recipe's control data, licenses and dependency declarations.
# The caller may have applied UPX to the executable after the Termux build.
if [ "$TARGET_OS" = android ]; then
    dpkg-deb -R "target/$TARGET_TRIPLE/release/termux-package.deb" "$root"
    install -m700 "target/$TARGET_TRIPLE/release/gproxy" \
        "$root/data/data/com.termux/files/usr/bin/gproxy"
    installed_size="$(du -sk --exclude=DEBIAN "$root" | cut -f1)"
    sed -i "s/^Installed-Size:.*/Installed-Size: $installed_size/" "$root/DEBIAN/control"
    if [ "${GPROXY_HEADLESS:-false}" = true ]; then
        # Same `gproxy` command as the full package, so install one or the other.
        sed -i 's/^Package: gproxy$/Package: gproxy-headless/' "$root/DEBIAN/control"
        printf 'Conflicts: gproxy\nProvides: gproxy\n' >> "$root/DEBIAN/control"
        grep -qx 'Package: gproxy-headless' "$root/DEBIAN/control"
    fi
    python3 scripts/reproducible-env.py --normalize-tree "$root"
    dpkg-deb --root-owner-group --build "$root" "$output/$ARTIFACT_NAME.deb"
    (cd "$output" && sha256sum "$ARTIFACT_NAME.deb" > "$ARTIFACT_NAME.deb.sha256")
    exit 0
fi
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
python3 scripts/reproducible-env.py --normalize-tree "$root"
dpkg-deb --root-owner-group --build "$root" "$output/$ARTIFACT_NAME.deb"
(cd "$output" && sha256sum "$ARTIFACT_NAME.deb" > "$ARTIFACT_NAME.deb.sha256")
