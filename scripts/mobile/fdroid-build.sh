#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../reproducible-env.sh"
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
build_root=/tmp/gproxy-reproduce
tools_dir="$build_root/tools"
export RUSTUP_HOME="$tools_dir/rustup" CARGO_HOME="$build_root/cargo"
export CARGO_TARGET_DIR="$build_root/source/target"
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64
export SOURCE_DATE_EPOCH="$(git show -s --format=%ct HEAD)"
# Preserve fdroidserver's scanned source tree, including removed wrapper JARs.
# Do not check out fresh files from Git after the source scan.
test -d "$CARGO_HOME" && test ! -e "$build_root/source"
trap 'rm -rf "$build_root"' EXIT
cp -a "$root" "$build_root/source"
ln -s "$ANDROID_NDK_HOME" "$build_root/android-ndk"
export ANDROID_NDK_HOME="$build_root/android-ndk" ANDROID_NDK_ROOT="$build_root/android-ndk" NDK_HOME="$build_root/android-ndk"
cd "$build_root/source"
export PATH="$JAVA_HOME/bin:$tools_dir/gproxy-android-upx/bin:$CARGO_HOME/bin:$tools_dir/node-v24.21.0-linux-x64/bin:$tools_dir/pnpm/bin:$tools_dir/go/bin:$tools_dir/gradle-8.14.3/bin:$PATH"
# fdroidserver removes upstream wrapper scripts/JARs during source scanning.
# Use the checksum-verified Gradle distribution installed by prebuild instead.
printf '#!/usr/bin/env bash\nexec gradle "$@"\n' > crates/gproxy-host-tauri/gen/android/gradlew
chmod +x crates/gproxy-host-tauri/gen/android/gradlew
pnpm --dir console install --frozen-lockfile
pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
bash scripts/mobile/build-android.sh fdroid aarch64
mkdir -p "$root/dist/mobile/fdroid/aarch64"
cp dist/mobile/fdroid/aarch64/gproxy-fdroid-aarch64.apk "$root/dist/mobile/fdroid/aarch64/"
