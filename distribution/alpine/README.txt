GPROXY packages for Alpine Linux (musl)

Built against Alpine edge. Requires apk-tools 3 and compatible runtime libraries.
These are Alpine Linux packages, not Android APKs.

First install the GPROXY public signing key as root. It is available in the
musl ZIP packages and in the source repository at:
  distribution/alpine/gproxy-alpine.rsa.pub
Install it with:
  install -Dm644 gproxy-alpine.rsa.pub /etc/apk/keys/gproxy-alpine.rsa.pub

Then install the downloaded package as root (signature verification is enabled):
  apk add ./gproxy-tauri-linux-<arch>-musl.apk
  apk add ./gproxy-linux-<arch>-musl.apk
  apk add ./gproxy-headless-linux-<arch>-musl.apk

Choose either the full CLI or the headless CLI: both install /usr/bin/gproxy
and explicitly conflict. Either can coexist with the desktop Application.
The CLI binaries are fully static. The Application declares its graphical
runtime dependencies; apk installs those automatically. Run gproxy-desktop
from a graphical desktop session or the application menu.

For the ZIP, install gtk+3.0, webkit2gtk-4.1, libayatana-appindicator and
librsvg, then run ./usr/bin/gproxy-desktop. This is not a fully static binary.
