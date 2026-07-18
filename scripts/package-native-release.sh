#!/usr/bin/env bash
# Package a Unix release build into the zip shape published by release.yml.
set -euo pipefail

target="${TARGET_TRIPLE:?missing TARGET_TRIPLE}"
target_os="${TARGET_OS:?missing TARGET_OS}"
artifact="${ARTIFACT_NAME:?missing ARTIFACT_NAME}"
binary="target/$target/release/gproxy"
android_package_base="${ANDROID_APK_PACKAGE_BASE:-io.github.leenhawk.gproxy}"
android_app_label="${ANDROID_APP_LABEL:-GPROXY}"
package_dir=""
output_dir="$PWD"

find_android_libcxx() {
  local target="$1"
  local ndk_root=""
  local candidate
  for candidate in "${ANDROID_NDK_ROOT:-}" "${ANDROID_NDK_HOME:-}"; do
    if [ -n "$candidate" ] && [ -d "$candidate" ]; then
      ndk_root="$candidate"
      break
    fi
  done

  if [ -z "$ndk_root" ]; then
    local sdk_root
    for sdk_root in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}"; do
      if [ -n "$sdk_root" ] && [ -d "$sdk_root/ndk" ]; then
        ndk_root="$(find "$sdk_root/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
        break
      fi
    done
  fi

  if [ -z "$ndk_root" ] || [ ! -d "$ndk_root" ]; then
    echo "could not locate Android NDK; set ANDROID_NDK_ROOT or ANDROID_HOME" >&2
    exit 1
  fi

  local prebuilt="$ndk_root/toolchains/llvm/prebuilt"
  if [ ! -d "$prebuilt" ]; then
    echo "missing Android NDK LLVM prebuilt directory under $ndk_root" >&2
    exit 1
  fi

  local libcxx
  libcxx="$(find "$prebuilt" \
    -path "*/sysroot/usr/lib/$target/libc++_shared.so" \
    -type f | sort | head -1)"
  if [ -z "$libcxx" ] || [ ! -f "$libcxx" ]; then
    echo "could not locate libc++_shared.so for $target under $ndk_root" >&2
    exit 1
  fi
  printf '%s\n' "$libcxx"
}

write_android_launcher() {
  local out="$1"
  cat > "$out" <<'EOF'
#!/system/bin/sh
set -eu

self="$0"
case "$self" in
  */*) dir="${self%/*}" ;;
  *)
    resolved="$(command -v "$self" 2>/dev/null || true)"
    case "$resolved" in
      */*) dir="${resolved%/*}" ;;
      *) dir="." ;;
    esac
    ;;
esac

case "${LD_LIBRARY_PATH:-}" in
  "") export LD_LIBRARY_PATH="$dir" ;;
  *) export LD_LIBRARY_PATH="$dir:$LD_LIBRARY_PATH" ;;
esac

exec "$dir/gproxy.bin" "$@"
EOF
  chmod 755 "$out"
}

find_android_sdk_root() {
  local sdk_root
  for sdk_root in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}"; do
    if [ -n "$sdk_root" ] && [ -d "$sdk_root" ]; then
      printf '%s\n' "$sdk_root"
      return 0
    fi
  done
  echo "could not locate Android SDK; set ANDROID_HOME or ANDROID_SDK_ROOT" >&2
  exit 1
}

find_android_platform_jar() {
  local sdk_root="$1"
  local android_jar
  android_jar="$(find "$sdk_root/platforms" -maxdepth 2 -name android.jar -type f 2>/dev/null | sort -V | tail -1)"
  if [ -z "$android_jar" ] || [ ! -f "$android_jar" ]; then
    echo "could not locate android.jar under $sdk_root/platforms" >&2
    exit 1
  fi
  printf '%s\n' "$android_jar"
}

find_android_build_tool() {
  local sdk_root="$1"
  local tool="$2"
  local path
  path="$(find "$sdk_root/build-tools" -maxdepth 2 -name "$tool" -type f 2>/dev/null | sort -V | tail -1)"
  if [ -z "$path" ] || [ ! -x "$path" ]; then
    echo "could not locate Android build tool '$tool' under $sdk_root/build-tools" >&2
    exit 1
  fi
  printf '%s\n' "$path"
}

find_d8() {
  local sdk_root="$1"
  local path
  path="$(find "$sdk_root" -name d8 -type f 2>/dev/null | sort -V | tail -1)"
  if [ -z "$path" ] || [ ! -x "$path" ]; then
    echo "could not locate Android build tool 'd8' under $sdk_root" >&2
    exit 1
  fi
  printf '%s\n' "$path"
}

require_android_release_signing() {
  case "${ANDROID_REQUIRE_SIGNING:-}" in
    1|true|TRUE|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

validate_android_package_name() {
  local package_name="$1"
  if [[ ! "$package_name" =~ ^[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+$ ]]; then
    echo "invalid Android package name: $package_name" >&2
    exit 1
  fi
}

android_package_name_for_target() {
  local package_name="${ANDROID_APK_PACKAGE_NAME:-}"
  if [ -z "$package_name" ]; then
    package_name="$android_package_base"
    if [ "${ANDROID_APK_PER_ABI_PACKAGE:-0}" = "1" ]; then
      package_name="$package_name.$(android_package_suffix_for_target "$1")"
    fi
  fi
  validate_android_package_name "$package_name"
  printf '%s\n' "$package_name"
}

android_abi_for_target() {
  case "$1" in
    aarch64-linux-android) printf '%s\n' "arm64-v8a" ;;
    x86_64-linux-android) printf '%s\n' "x86_64" ;;
    *) echo "unsupported Android target for APK packaging: $1" >&2; exit 1 ;;
  esac
}

android_package_suffix_for_target() {
  case "$1" in
    aarch64-linux-android) printf '%s\n' "arm64" ;;
    x86_64-linux-android) printf '%s\n' "x64" ;;
    *) echo "unsupported Android target for APK packaging: $1" >&2; exit 1 ;;
  esac
}

android_cargo_version() {
  awk '
    /^\[package\]/ { in_package = 1; next }
    /^\[/ && in_package { exit }
    in_package && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' Cargo.toml
}

android_version_name() {
  local version_name="${ANDROID_VERSION_NAME:-}"
  if [ -z "$version_name" ]; then
    version_name="$(android_cargo_version)"
  fi
  version_name="${version_name#v}"
  if [ -z "$version_name" ]; then
    echo "could not determine Android versionName; set ANDROID_VERSION_NAME" >&2
    exit 1
  fi
  printf '%s\n' "$version_name"
}

android_version_code_from_name() {
  local version_name="$1"
  if [[ "$version_name" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+) ]]; then
    printf '%s\n' "$((10#${BASH_REMATCH[1]} * 1000000 + 10#${BASH_REMATCH[2]} * 1000 + 10#${BASH_REMATCH[3]}))"
  else
    printf '%s\n' "1"
  fi
}

android_version_code() {
  local version_name="$1"
  local version_code="${ANDROID_VERSION_CODE:-}"
  if [ -z "$version_code" ]; then
    version_code="$(android_version_code_from_name "$version_name")"
  fi
  if [[ ! "$version_code" =~ ^[0-9]+$ ]] || [ "$version_code" -lt 1 ] || [ "$version_code" -gt 2100000000 ]; then
    echo "invalid Android versionCode: $version_code" >&2
    exit 1
  fi
  printf '%s\n' "$version_code"
}

xml_escape() {
  local value="$1"
  value="${value//&/&amp;}"
  value="${value//</&lt;}"
  value="${value//>/&gt;}"
  value="${value//\"/&quot;}"
  printf '%s\n' "$value"
}

prepare_android_keystore() {
  local work="$1"
  local keystore="$work/signing.keystore"

  if [ -n "${ANDROID_SIGNING_KEYSTORE_B64:-}" ]; then
    : "${ANDROID_SIGNING_KEYSTORE_PASSWORD:?missing ANDROID_SIGNING_KEYSTORE_PASSWORD}"
    printf '%s' "$ANDROID_SIGNING_KEYSTORE_B64" | base64 -d > "$keystore"
    printf '%s\n' "$keystore"
    return 0
  fi

  if [ -n "${ANDROID_SIGNING_KEYSTORE:-}" ]; then
    : "${ANDROID_SIGNING_KEYSTORE_PASSWORD:?missing ANDROID_SIGNING_KEYSTORE_PASSWORD}"
    if [ ! -f "$ANDROID_SIGNING_KEYSTORE" ]; then
      echo "missing Android signing keystore: $ANDROID_SIGNING_KEYSTORE" >&2
      exit 1
    fi
    printf '%s\n' "$ANDROID_SIGNING_KEYSTORE"
    return 0
  fi

  if require_android_release_signing; then
    echo "missing Android release signing key; set ANDROID_SIGNING_KEYSTORE_B64 or ANDROID_SIGNING_KEYSTORE" >&2
    exit 1
  fi

  echo "warning: signing Android APK with generated debug key" >&2
  keytool -genkeypair \
    -keystore "$keystore" \
    -storepass android \
    -keypass android \
    -alias androiddebugkey \
    -keyalg RSA \
    -keysize 2048 \
    -validity 10000 \
    -dname "CN=Android Debug,O=Android,C=US" >/dev/null
  printf '%s\n' "$keystore"
}

find_android_icon_source() {
  local candidate
  for candidate in \
    "${ANDROID_ICON_SOURCE:-}" \
    "console/public/favicon-96x96.png" \
    "docs/public/favicon-96x96.png" \
    "assets/console/favicon-96x96.png"; do
    if [ -n "$candidate" ] && [ -f "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  echo "could not locate Android icon source; set ANDROID_ICON_SOURCE" >&2
  exit 1
}

resize_android_icon() {
  local source="$1"
  local size="$2"
  local out="$3"
  if command -v magick >/dev/null 2>&1; then
    magick "$source" -resize "${size}x${size}" "$out"
  elif command -v convert >/dev/null 2>&1; then
    convert "$source" -resize "${size}x${size}" "$out"
  else
    cp "$source" "$out"
  fi
}

write_android_icons() {
  local res_dir="$1"
  local source
  source="$(find_android_icon_source)"

  local spec density size
  for spec in \
    "mipmap-mdpi:48" \
    "mipmap-hdpi:72" \
    "mipmap-xhdpi:96" \
    "mipmap-xxhdpi:144" \
    "mipmap-xxxhdpi:192"; do
    density="${spec%%:*}"
    size="${spec##*:}"
    mkdir -p "$res_dir/$density"
    resize_android_icon "$source" "$size" "$res_dir/$density/ic_launcher.png"
  done
}


compile_android_activity() {
  local work="$1"
  local package_name="$2"
  local android_jar="$3"
  local d8="$4"
  local min_sdk="$5"
  local source_dir="$work/src/${package_name//.//}"
  mkdir -p "$source_dir" "$work/classes" "$work/dex"
  local template output
  local sources=()
  for template in scripts/android/*.java.in; do
    output="$source_dir/$(basename "${template%.in}")"
    sed "s/__PACKAGE__/$package_name/g" "$template" > "$output"
    sources+=("$output")
  done
  javac -source 1.8 -target 1.8 -bootclasspath "$android_jar" \
    -d "$work/classes" "${sources[@]}"
  local classes=()
  while IFS= read -r class_file; do
    classes+=("$class_file")
  done < <(find "$work/classes" -name '*.class' -type f | sort)
  "$d8" --min-api "$min_sdk" --lib "$android_jar" --output "$work/dex" \
    "${classes[@]}"
}

sign_android_apk() {
  local apksigner="$1"
  local work="$2"
  local aligned="$3"
  local signed="$4"
  local keystore
  keystore="$(prepare_android_keystore "$work")"

  local storepass="${ANDROID_SIGNING_KEYSTORE_PASSWORD:-android}"
  local keypass="${ANDROID_SIGNING_KEY_PASSWORD:-}"
  local alias="${ANDROID_SIGNING_KEY_ALIAS:-}"
  local signer_args=()
  if [ -z "$alias" ] && [ -z "${ANDROID_SIGNING_KEYSTORE_B64:-}" ] && [ -z "${ANDROID_SIGNING_KEYSTORE:-}" ]; then
    alias="androiddebugkey"
    keypass="android"
  fi
  if [ -n "$alias" ]; then
    signer_args+=(--ks-key-alias "$alias")
  fi
  if [ -n "$keypass" ]; then
    signer_args+=(--key-pass "pass:$keypass")
  fi

  "$apksigner" sign \
    --v4-signing-enabled false \
    --ks "$keystore" \
    "${signer_args[@]}" \
    --ks-pass "pass:$storepass" \
    --out "$signed" \
    "$aligned"
  "$apksigner" verify --verbose "$signed" >/dev/null
}

package_android_apk() {
  local min_sdk="${ANDROID_MIN_SDK:-28}"
  local target_sdk="${ANDROID_TARGET_SDK:-28}"
  local version_name version_code app_label_xml version_name_xml
  version_name="$(android_version_name)"
  version_code="$(android_version_code "$version_name")"
  app_label_xml="$(xml_escape "$android_app_label")"
  version_name_xml="$(xml_escape "$version_name")"
  local abi package_name sdk_root android_jar aapt zipalign apksigner d8 work
  abi="$(android_abi_for_target "$target")"
  package_name="$(android_package_name_for_target "$target")"
  sdk_root="$(find_android_sdk_root)"
  android_jar="$(find_android_platform_jar "$sdk_root")"
  aapt="$(find_android_build_tool "$sdk_root" aapt)"
  zipalign="$(find_android_build_tool "$sdk_root" zipalign)"
  apksigner="$(find_android_build_tool "$sdk_root" apksigner)"
  d8="$(find_d8 "$sdk_root")"
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN

  mkdir -p "$work/assets/gproxy" "$work/native/lib/$abi" "$work/res/values"
  cp "$package_dir/gproxy" "$package_dir/gproxy.bin" "$package_dir/libc++_shared.so" README.md "$work/assets/gproxy/"
  cp "$package_dir/libc++_shared.so" "$work/native/lib/$abi/libc++_shared.so"
  write_android_icons "$work/res"
  compile_android_activity "$work" "$package_name" "$android_jar" "$d8" "$min_sdk"

  cat > "$work/res/values/strings.xml" <<EOF
<resources>
    <string name="app_name">$app_label_xml</string>
</resources>
EOF

  # The launcher health-checks the loopback Console over HTTP. Android 9+
  # blocks HttpURLConnection cleartext for target SDK 28 unless we opt in.
  cat > "$work/AndroidManifest.xml" <<EOF
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="$package_name"
    android:versionCode="$version_code"
    android:versionName="$version_name_xml">
    <uses-sdk android:minSdkVersion="$min_sdk" android:targetSdkVersion="$target_sdk" />
    <uses-permission android:name="android.permission.INTERNET" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
    <uses-permission android:name="android.permission.RECEIVE_BOOT_COMPLETED" />
    <uses-permission android:name="android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS" />
    <uses-permission android:name="android.permission.REQUEST_INSTALL_PACKAGES" />
    <application
        android:label="@string/app_name"
        android:icon="@mipmap/ic_launcher"
        android:roundIcon="@mipmap/ic_launcher"
        android:theme="@android:style/Theme.Material.Light.NoActionBar"
        android:usesCleartextTraffic="true"
        android:extractNativeLibs="true"
        android:allowBackup="false"
        android:supportsRtl="true">
        <activity
            android:name=".GproxyActivity"
            android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
        <activity
            android:name=".GproxyUpdateActivity"
            android:exported="false"
            android:excludeFromRecents="true"
            android:theme="@android:style/Theme.Material.Light.Dialog.Alert" />
        <service
            android:name=".GproxyService"
            android:exported="false"
            android:stopWithTask="false" />
        <provider
            android:name=".GproxyUpdateProvider"
            android:authorities="$package_name.updates"
            android:exported="false"
            android:grantUriPermissions="true" />
        <receiver
            android:name=".GproxyBootReceiver"
            android:enabled="true"
            android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.BOOT_COMPLETED" />
                <action android:name="android.intent.action.MY_PACKAGE_REPLACED" />
            </intent-filter>
        </receiver>
    </application>
</manifest>
EOF

  "$aapt" package \
    -f \
    -M "$work/AndroidManifest.xml" \
    -I "$android_jar" \
    -S "$work/res" \
    -A "$work/assets" \
    -F "$work/unsigned.apk" >/dev/null
  (cd "$work/dex" && zip -qr "$work/unsigned.apk" classes.dex)
  (cd "$work/native" && zip -qr "$work/unsigned.apk" lib)
  "$zipalign" -f -p 4 "$work/unsigned.apk" "$work/aligned.apk"
  sign_android_apk "$apksigner" "$work" "$work/aligned.apk" "$artifact.apk"
  shasum -a 256 "$artifact.apk" > "$artifact.apk.sha256"
  trap - RETURN
  rm -rf "$work"
}

if [ ! -f "$binary" ]; then
  echo "missing release binary: $binary" >&2
  exit 1
fi

package_dir="$(mktemp -d)"
trap 'rm -rf "$package_dir"' EXIT
cp README.md LICENSE "$package_dir/"

if [ "$target_os" = "android" ]; then
  cp "$binary" "$package_dir/gproxy.bin"
  chmod 755 "$package_dir/gproxy.bin"
  cp "$(find_android_libcxx "$target")" "$package_dir/libc++_shared.so"
  chmod 644 "$package_dir/libc++_shared.so"
  write_android_launcher "$package_dir/gproxy"
  (cd "$package_dir" && zip -9 "$output_dir/$artifact.zip" gproxy gproxy.bin libc++_shared.so README.md LICENSE)
  package_android_apk
else
  cp "$binary" "$package_dir/gproxy"
  chmod 755 "$package_dir/gproxy"
  (cd "$package_dir" && zip -9 "$output_dir/$artifact.zip" gproxy README.md LICENSE)
  if [ "$target_os" = "linux" ]; then
    bash scripts/package-linux-deb.sh
  elif [ "$target_os" = "macos" ]; then
    bash scripts/package-macos-dmg.sh
  fi
fi

shasum -a 256 "$artifact.zip" > "$artifact.zip.sha256"
