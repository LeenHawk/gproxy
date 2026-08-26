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
