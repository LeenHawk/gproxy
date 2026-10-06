#!/usr/bin/env bash
# A glibc desktop cannot load Alpine's GTK/WebKit libraries. Ship their musl
# runtime privately and enter it with bubblewrap, keeping the host network/home.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/reproducible-env.sh"
umask 022
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
apk_file="${1:?signed Application APK is required}"
: "${TARGET_TRIPLE:?}" "${ARTIFACT_NAME:?}" "${GPROXY_BUILD_VERSION:?}"
case "$TARGET_TRIPLE" in
  x86_64-unknown-linux-musl) arch=amd64; alpine_arch=x86_64 ;;
  aarch64-unknown-linux-musl) arch=arm64; alpine_arch=aarch64 ;;
  riscv64gc-unknown-linux-musl) arch=riscv64; alpine_arch=riscv64 ;;
  *) echo "unsupported musl DEB target: $TARGET_TRIPLE" >&2; exit 1 ;;
esac
output="${OUTPUT_DIR:-$root/dist/release}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
package="$work/package"
runtime="$package/opt/gproxy-desktop-musl/rootfs"
mkdir -p "$runtime/etc/apk/keys" "$package/DEBIAN" "$output"
cp /usr/share/apk/keys/"$alpine_arch"/*.pub distribution/alpine/gproxy-alpine.rsa.pub "$runtime/etc/apk/keys/"
cp /etc/apk/repositories "$runtime/etc/apk/"
# Cross packaging must not execute foreign maintainer scripts. Generate the
# graphical caches explicitly below with host tools or the RISC-V emulator.
trust_args=()
if [ "${GPROXY_UNSIGNED_BUILD:-0}" = 1 ]; then trust_args=(--allow-untrusted); fi
fakeroot apk "${trust_args[@]}" --root "$runtime" --arch "$alpine_arch" --initdb --no-scripts \
  add --no-cache alpine-baselayout ca-certificates-bundle "$apk_file"
# This is a build-time installation log, not part of the application runtime.
rm -f "$runtime/var/log/apk.log"
glib-compile-schemas "$runtime/usr/share/glib-2.0/schemas"
pixbuf_dir="$(find "$runtime/usr/lib/gdk-pixbuf-2.0" -type d -name loaders -print -quit)"
query="$runtime/usr/bin/gdk-pixbuf-query-loaders"
runner=()
if [ "$alpine_arch" = riscv64 ]; then
  runner=(qemu-riscv64 -L "$runtime")
fi
"${runner[@]}" "$runtime/usr/bin/gio-querymodules" "$runtime/usr/lib/gio/modules"
test -s "$runtime/usr/lib/gio/modules/giomodule.cache"
GDK_PIXBUF_MODULEDIR="$pixbuf_dir" "${runner[@]}" "$query" > "$work/loaders.cache"
sed "s|$runtime||g" "$work/loaders.cache" > "${pixbuf_dir%/loaders}/loaders.cache"
# /etc is read-only at launch; provide mount points for host identity/network.
for file in resolv.conf hosts localtime machine-id passwd group; do
  [ -e "$runtime/etc/$file" ] || touch "$runtime/etc/$file"
done
mkdir -p "$runtime/usr/share/fonts" "$runtime/usr/local/share/fonts"
install -Dm755 distribution/alpine/gproxy-musl-launcher.sh "$package/usr/bin/gproxy-desktop-musl"
install -Dm644 distribution/alpine/gproxy-musl.desktop "$package/usr/share/applications/gproxy-musl.desktop"
install -Dm644 crates/gproxy-host-tauri/icons/icon.png "$package/usr/share/icons/hicolor/512x512/apps/gproxy-musl.png"
install -Dm644 LICENSE "$package/usr/share/doc/gproxy-desktop-musl/copyright"
install -m644 distribution/alpine/README.txt "$package/usr/share/doc/gproxy-desktop-musl/README.txt"
size="$(du -sk "$package/opt" "$package/usr" | awk '{sum += $1} END {print sum}')"
cat > "$package/DEBIAN/control" <<CONTROL
Package: gproxy-desktop-musl
Version: ${GPROXY_BUILD_VERSION/-/\~}
Architecture: $arch
Maintainer: Leen Hawk <leenhawk@leenhawk.com>
Section: net
Priority: optional
Depends: bubblewrap, ca-certificates, fonts-dejavu-core
Installed-Size: $size
Homepage: https://github.com/LeenHawk/gproxy
Description: GPROXY desktop application with an Alpine musl runtime
 Includes musl GTK and WebKitGTK libraries in a private runtime.
 Uses bubblewrap with the host display, home directory and network.
CONTROL
python3 scripts/reproducible-env.py --normalize-tree "$package"
dpkg-deb --root-owner-group --build "$package" "$output/$ARTIFACT_NAME.deb"
chmod 644 "$output/$ARTIFACT_NAME.deb"
(cd "$output" && sha256sum "$ARTIFACT_NAME.deb" > "$ARTIFACT_NAME.deb.sha256")
chmod 644 "$output/$ARTIFACT_NAME.deb.sha256"
