GPROXY Android CLI for Termux
=============================

This archive contains a Termux executable. It requires the standard Termux
installation prefix: /data/data/com.termux/files/usr. Run it inside Termux,
not an ordinary adb shell or another Android terminal application.

1. Install the runtime dependencies inside Termux:

   pkg update
   pkg install libc++ openssl ca-certificates

2. Extract this ZIP into a directory in Termux's home directory. Android's
   shared storage (for example /sdcard/Download) cannot execute these files.
   If necessary, run: chmod +x gproxy gproxy.bin

3. Start the server:

   ./gproxy serve --data-dir "$HOME/.local/share/gproxy"

   Keep your data directory, configuration and master key when upgrading.
   The Console is served at http://127.0.0.1:8787/console/ by default.

Updates
-------

Self-update and rollback from the CLI/Console are disabled in this build.
Prefer the matching .deb attachment and install it with Termux's APT:

   apt install ./gproxy-android-<architecture>.deb

After switching from this ZIP to the package, run `gproxy` from $PREFIX/bin
with the same data directory, configuration and master key. Update existing
Termux:Boot scripts if they still point to the executable extracted from ZIP.

Until the package is included in an enabled APT repository, download each
new release's DEB and use `apt install ./new-package.deb`. Once an enabled
repository provides it, use `pkg upgrade gproxy`.

The ZIP does not bundle private copies of OpenSSL or the C++ runtime. Their
security updates come from `pkg upgrade`. The Tauri Android APK is a separate
application and does not use this Termux package.
