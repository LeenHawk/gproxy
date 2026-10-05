#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../reproducible-env.sh"
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
tools_dir="$HOME/.local/share/gproxy-fdroid"
export RUSTUP_HOME="$tools_dir/rustup" CARGO_HOME="$tools_dir/cargo"
export PATH="$tools_dir/gproxy-android-upx/bin:$tools_dir/cargo/bin:$tools_dir/node-v24.21.0-linux-x64/bin:$tools_dir/pnpm/bin:$tools_dir/go/bin:$tools_dir/gradle-8.14.3/bin:$PATH"
# fdroidserver removes upstream wrapper scripts/JARs during source scanning.
# Use the checksum-verified Gradle distribution installed by prebuild instead.
printf '#!/usr/bin/env bash\nexec gradle "$@"\n' > crates/gproxy-host-tauri/gen/android/gradlew
chmod +x crates/gproxy-host-tauri/gen/android/gradlew
pnpm --dir console install --frozen-lockfile
pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
bash scripts/mobile/build-android.sh fdroid aarch64
