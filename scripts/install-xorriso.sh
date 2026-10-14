#!/usr/bin/env bash
set -euo pipefail
prefix="${RUNNER_TEMP:?}/gproxy-xorriso-1.5.6-gproxy2"
if [ ! -x "$prefix/bin/xorriso" ]; then
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  curl -fsSL --connect-timeout 15 --retry 2 https://mirrors.kernel.org/gnu/xorriso/xorriso-1.5.6.tar.gz -o "$work/source.tar.gz"
  printf '%s  %s\n' d4b6b66bd04c49c6b358ee66475d806d6f6d7486e801106a47d331df1f2f8feb "$work/source.tar.gz" | shasum -a 256 -c -
  tar -xzf "$work/source.tar.gz" -C "$work"
  (
    cd "$work/xorriso-1.5.6"
    # libisofs writes legacy ???? type/creator codes on ordinary HFS+ files.
    # macOS exposes these as FinderInfo, which invalidates sealed app resources.
    # Leave the fields zero, as on a normal clean signed .app; retain symlink codes.
    python3 - <<'PY'
from pathlib import Path
path = Path("libisofs/hfsplus.c")
source = path.read_text()
for field in ("file_type", "file_creator"):
    old = f'memcpy (common->{field}, "????", 4);'
    assert source.count(old) == 1
    source = source.replace(old, f'memset (common->{field}, 0, 4);')
# The final allocation byte covers 1..8 blocks, never zero. Otherwise an
# eight-block boundary marks live data free and hdiutil zeroes it during UDZO
# conversion. Also write the final byte when it starts a new bitmap block.
old = '''    if (over)
      {
\tmemset (buffer + over, 0, sizeof (buffer) - over);
\tbuffer[over] = 0xff00 >> (t->hfsp_total_blocks % 8);'''
new = '''      {
\tmemset (buffer + over, 0, sizeof (buffer) - over);
\tbuffer[over] = 0xff00 >> (((t->hfsp_total_blocks - 1) % 8) + 1);'''
assert source.count(old) == 1
source = source.replace(old, new)
path.write_text(source)
PY
    CPPFLAGS="${CPPFLAGS:-} -include sys/types.h" ./configure --prefix="$prefix" --disable-libacl --disable-xattr
    make -j2
    make install
  )
fi
"$prefix/bin/xorriso" -version
printf '%s\n' "$prefix/bin" >> "${GITHUB_PATH:?}"
