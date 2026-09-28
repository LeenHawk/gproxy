#!/usr/bin/env bash
set -euo pipefail
export ANDROID_HOME="$CI_PROJECT_DIR/.ci-tools/android"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/30.0.15729638" NDK_HOME="$ANDROID_HOME/ndk/30.0.15729638"
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64
apt-get install -y --no-install-recommends openjdk-21-jdk-headless
mkdir -p "$ANDROID_HOME/cmdline-tools"
curl -fsSL --retry 5 --retry-all-errors https://dl.google.com/android/repository/commandlinetools-linux-16111833_latest.zip -o "$ANDROID_HOME/tools.zip"
echo "e025545c62a8e64c7559119566a569fb1dec5f60  $ANDROID_HOME/tools.zip" | sha1sum -c -
unzip -q "$ANDROID_HOME/tools.zip" -d "$ANDROID_HOME/cmdline-tools"
mv "$ANDROID_HOME/cmdline-tools/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"
# `yes` exits with SIGPIPE once sdkmanager has consumed the license answers.
set +o pipefail
yes | sdkmanager --licenses >/dev/null
set -o pipefail
sdkmanager 'platform-tools' 'platforms;android-36' 'build-tools;36.1.0' 'ndk;30.0.15729638'
cargo install cargo-ndk --locked --version 4.1.2
