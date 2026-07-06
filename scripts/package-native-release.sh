#!/usr/bin/env bash
# Package a Unix release build into the zip shape published by release.yml.
set -euo pipefail

target="${TARGET_TRIPLE:?missing TARGET_TRIPLE}"
target_os="${TARGET_OS:?missing TARGET_OS}"
artifact="${ARTIFACT_NAME:?missing ARTIFACT_NAME}"
binary="target/$target/release/gproxy"
android_package_base="${ANDROID_APK_PACKAGE_BASE:-io.github.leenhawk.gproxy}"
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

prepare_android_keystore() {
  local work="$1"
  local keystore="$work/signing.keystore"

  if [ -n "${ANDROID_SIGNING_KEYSTORE_B64:-}" ]; then
    : "${ANDROID_SIGNING_KEYSTORE_PASSWORD:?missing ANDROID_SIGNING_KEYSTORE_PASSWORD}"
    printf '%s' "$ANDROID_SIGNING_KEYSTORE_B64" | base64 -d > "$keystore"
    printf '%s\n' "$keystore"
    return 0
  fi

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

write_android_activity() {
  local source="$1"
  local package_name="$2"
  cat > "$source" <<EOF
package $package_name;

import android.app.Activity;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.text.InputType;
import android.view.View;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.Map;

public final class GproxyActivity extends Activity {
    private final Handler handler = new Handler(Looper.getMainLooper());
    private EditText adminUserInput;
    private EditText adminPasswordInput;
    private TextView logView;
    private Process process;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        int pad = dp(16);
        root.setPadding(pad, pad, pad, pad);

        adminUserInput = new EditText(this);
        adminUserInput.setHint("Admin username");
        adminUserInput.setSingleLine(true);
        adminUserInput.setText("admin");

        adminPasswordInput = new EditText(this);
        adminPasswordInput.setHint("Admin password");
        adminPasswordInput.setSingleLine(true);
        adminPasswordInput.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD);

        Button start = new Button(this);
        start.setText("Start GPROXY");
        Button stop = new Button(this);
        stop.setText("Stop");

        logView = new TextView(this);
        logView.setTextIsSelectable(true);
        log("GPROXY APK installed.");
        log("Tap Start GPROXY, then open http://127.0.0.1:8787/console");

        start.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                startGproxy();
            }
        });
        stop.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                stopGproxy();
            }
        });

        root.addView(adminUserInput);
        root.addView(adminPasswordInput);
        root.addView(start);
        root.addView(stop);
        ScrollView scroll = new ScrollView(this);
        scroll.addView(logView);
        root.addView(scroll, new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, 0, 1.0f));
        setContentView(root);
    }

    private void startGproxy() {
        if (isGproxyRunning()) {
            log("GPROXY is already running.");
            return;
        }
        try {
            String adminUser = adminUserInput.getText().toString().trim();
            String adminPassword = adminPasswordInput.getText().toString();
            if (adminUser.length() == 0) {
                adminUser = "admin";
            }

            File binDir = new File(getFilesDir(), "bin");
            File dataDir = new File(getFilesDir(), "data");
            if (!binDir.isDirectory() && !binDir.mkdirs()) {
                throw new IOException("create " + binDir);
            }
            if (!dataDir.isDirectory() && !dataDir.mkdirs()) {
                throw new IOException("create " + dataDir);
            }
            File executable = copyAsset("gproxy/gproxy.bin", new File(binDir, "gproxy"), true);
            File libcxx = copyAsset("gproxy/libc++_shared.so", new File(binDir, "libc++_shared.so"), false);
            log("Executable: " + executable.getAbsolutePath());

            ArrayList<String> command = new ArrayList<String>();
            command.add(executable.getAbsolutePath());
            command.add("--host");
            command.add("127.0.0.1");
            command.add("--port");
            command.add("8787");
            command.add("--data-dir");
            command.add(dataDir.getAbsolutePath());
            command.add("--admin-user");
            command.add(adminUser);
            if (adminPassword.length() > 0) {
                command.add("--admin-password");
                command.add(adminPassword);
            }

            ProcessBuilder builder = new ProcessBuilder(command);
            Map<String, String> env = builder.environment();
            env.put("LD_LIBRARY_PATH", libcxx.getParentFile().getAbsolutePath());
            builder.redirectErrorStream(true);
            process = builder.start();
            adminUserInput.setEnabled(false);
            adminPasswordInput.setEnabled(false);
            log("Started. Console: http://127.0.0.1:8787/console");
            readOutput(process);
        } catch (Exception e) {
            log("Start failed: " + e);
        }
    }

    private void stopGproxy() {
        if (!isGproxyRunning()) {
            log("GPROXY is not running.");
            return;
        }
        process.destroy();
        log("Stopping GPROXY.");
    }

    private boolean isGproxyRunning() {
        if (process == null) {
            return false;
        }
        try {
            process.exitValue();
            return false;
        } catch (IllegalThreadStateException e) {
            return true;
        }
    }

    private File copyAsset(String assetName, File out, boolean executable) throws IOException {
        InputStream input = getAssets().open(assetName);
        try {
            FileOutputStream output = new FileOutputStream(out);
            try {
                byte[] buffer = new byte[64 * 1024];
                int read;
                while ((read = input.read(buffer)) != -1) {
                    output.write(buffer, 0, read);
                }
            } finally {
                output.close();
            }
        } finally {
            input.close();
        }
        out.setReadable(true, true);
        out.setWritable(true, true);
        out.setExecutable(executable, true);
        return out;
    }

    private void readOutput(final Process running) {
        new Thread(new Runnable() {
            @Override
            public void run() {
                try {
                    InputStream input = running.getInputStream();
                    byte[] buffer = new byte[4096];
                    int read;
                    while ((read = input.read(buffer)) != -1) {
                        final String text = new String(buffer, 0, read);
                        handler.post(new Runnable() {
                            @Override
                            public void run() {
                                logView.append(text);
                            }
                        });
                    }
                    final int code = running.waitFor();
                    handler.post(new Runnable() {
                        @Override
                        public void run() {
                            log("GPROXY exited with code " + code + ".");
                            if (process == running) {
                                process = null;
                            }
                            adminUserInput.setEnabled(true);
                            adminPasswordInput.setEnabled(true);
                        }
                    });
                } catch (Exception e) {
                    final String message = e.toString();
                    handler.post(new Runnable() {
                        @Override
                        public void run() {
                            log("Output reader failed: " + message);
                            if (process == running) {
                                process = null;
                            }
                            adminUserInput.setEnabled(true);
                            adminPasswordInput.setEnabled(true);
                        }
                    });
                }
            }
        }, "gproxy-output").start();
    }

    private void log(String line) {
        logView.append(line + "\\n");
    }

    private int dp(int value) {
        float density = getResources().getDisplayMetrics().density;
        return (int) (value * density + 0.5f);
    }
}
EOF
}

compile_android_activity() {
  local work="$1"
  local package_name="$2"
  local android_jar="$3"
  local d8="$4"
  local min_sdk="$5"
  local source_dir="$work/src/${package_name//.//}"
  mkdir -p "$source_dir" "$work/classes" "$work/dex"
  write_android_activity "$source_dir/GproxyActivity.java" "$package_name"
  javac -source 1.8 -target 1.8 -bootclasspath "$android_jar" \
    -d "$work/classes" "$source_dir/GproxyActivity.java"
  "$d8" --min-api "$min_sdk" --lib "$android_jar" --output "$work/dex" \
    $(find "$work/classes" -name '*.class' -type f | sort)
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
  if [ -z "$alias" ] && [ -z "${ANDROID_SIGNING_KEYSTORE_B64:-}" ]; then
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
  local min_sdk="${ANDROID_MIN_SDK:-21}"
  local target_sdk="${ANDROID_TARGET_SDK:-28}"
  local version_code="${ANDROID_VERSION_CODE:-1}"
  local version_name="${ANDROID_VERSION_NAME:-0.0.0}"
  local suffix abi package_name sdk_root android_jar aapt zipalign apksigner d8 work
  suffix="$(android_package_suffix_for_target "$target")"
  abi="$(android_abi_for_target "$target")"
  package_name="$android_package_base.$suffix"
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
  compile_android_activity "$work" "$package_name" "$android_jar" "$d8" "$min_sdk"

  cat > "$work/res/values/strings.xml" <<EOF
<resources>
    <string name="app_name">GPROXY</string>
</resources>
EOF

  cat > "$work/AndroidManifest.xml" <<EOF
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="$package_name"
    android:versionCode="$version_code"
    android:versionName="$version_name">
    <uses-sdk android:minSdkVersion="$min_sdk" android:targetSdkVersion="$target_sdk" />
    <uses-permission android:name="android.permission.INTERNET" />
    <application
        android:label="@string/app_name"
        android:theme="@android:style/Theme.Material.Light.NoActionBar"
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
cp README.md "$package_dir/"

if [ "$target_os" = "android" ]; then
  cp "$binary" "$package_dir/gproxy.bin"
  chmod 755 "$package_dir/gproxy.bin"
  cp "$(find_android_libcxx "$target")" "$package_dir/libc++_shared.so"
  chmod 644 "$package_dir/libc++_shared.so"
  write_android_launcher "$package_dir/gproxy"
  (cd "$package_dir" && zip -9 "$output_dir/$artifact.zip" gproxy gproxy.bin libc++_shared.so README.md)
  package_android_apk
else
  cp "$binary" "$package_dir/gproxy"
  chmod 755 "$package_dir/gproxy"
  (cd "$package_dir" && zip -9 "$output_dir/$artifact.zip" gproxy README.md)
fi

shasum -a 256 "$artifact.zip" > "$artifact.zip.sha256"
