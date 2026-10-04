#!/usr/bin/env bash
set -euo pipefail
runtime=/opt/gproxy-desktop-musl/rootfs
args=(/usr/bin/bwrap --die-with-parent --new-session)
# Construct a private root instead of replacing the host's libc or GTK files.
for directory in bin etc lib sbin usr var; do
  args+=(--ro-bind "$runtime/$directory" "/$directory")
done
args+=(--dev-bind /dev /dev --proc /proc --ro-bind /sys /sys
  --ro-bind /etc/ssl/certs /etc/ssl/certs
  --ro-bind-try /usr/share/fonts /usr/share/fonts
  --ro-bind-try /usr/local/share/fonts /usr/local/share/fonts
  --bind /tmp /tmp --bind /run /run --bind "$HOME" "$HOME" --chdir "$HOME")
for file in resolv.conf hosts localtime machine-id passwd group; do
  args+=(--ro-bind-try "/etc/$file" "/etc/$file")
done
for directory in "${XDG_CONFIG_HOME:-$HOME/.config}" "${XDG_DATA_HOME:-$HOME/.local/share}" "${XDG_CACHE_HOME:-$HOME/.cache}"; do
  mkdir -p "$directory"
  args+=(--bind "$directory" "$directory")
done
exec "${args[@]}" --setenv PATH /usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin \
  --setenv GPROXY_DESKTOP_LAUNCHER /usr/bin/gproxy-desktop-musl \
  --unsetenv LD_LIBRARY_PATH --unsetenv LD_PRELOAD --unsetenv GTK_PATH \
  --unsetenv GIO_EXTRA_MODULES --unsetenv GDK_PIXBUF_MODULE_FILE \
  -- /usr/bin/gproxy-desktop "$@"
