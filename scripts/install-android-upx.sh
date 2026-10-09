#!/usr/bin/env bash
# UPX 5.2.1 cannot pack our ARM64 shared library (upx/upx#18916).
# Use the upstream Android shared-library stub fix until it is released.
# UPX also mishandles x86_64 --android-shlib output: it lowers slid PT_LOADs
# to 4 KiB alignment (Android requires 16 KiB) and leaves e_shoff pointing
# past EOF, which bionic rejects with "invalid shdr offset/size". Keep the
# linked alignment and forward section headers as UPX already does for ARM64
# (upx/upx#18936).
set -euo pipefail
revision=d5faee0886bb35ea24d7ff07b5bab518b9153796
source_dir="${1:-${RUNNER_TEMP:?}/gproxy-android-upx}"
git init "$source_dir"
if ! git -C "$source_dir" remote get-url origin >/dev/null 2>&1; then
  git -C "$source_dir" remote add origin https://github.com/upx/upx.git
fi
git -C "$source_dir" fetch --depth 1 origin "$revision"
git -C "$source_dir" checkout --force --detach FETCH_HEAD
git -C "$source_dir" submodule update --init --recursive --depth 1
git -C "$source_dir" apply - <<'PATCH'
diff --git a/src/p_lx_elf.cpp b/src/p_lx_elf.cpp
index 8d4939e..be362f7 100644
--- a/src/p_lx_elf.cpp
+++ b/src/p_lx_elf.cpp
@@ -877,6 +877,7 @@ off_t PackLinuxElf64::pack3(OutputFile *fo, Filter &ft)
                 else if (xct_off < ioff) { // Slide subsequent PT_LOAD.
                     if ((1u<<12) < align
                     &&  Elf64_Ehdr::EM_X86_64 == e_machine  // FIXME: other $ARCH ?
+                    &&  !saved_opt_android_shlib  // Android requires 16 KiB PT_LOAD
                     ) {
                         align = 1u<<12;
                         set_te64(&phdr->p_align, align);
@@ -6487,7 +6488,8 @@ unsigned PackLinuxElf64::forward_Shdrs(OutputFile *fo, Elf64_Ehdr *const eho)
         return 0;
     }
     unsigned penalty = total_out;
-    if (Elf64_Ehdr::EM_AARCH64 == e_machine
+    if ((Elf64_Ehdr::EM_AARCH64 == e_machine
+      || Elf64_Ehdr::EM_X86_64 == e_machine)
     &&  saved_opt_android_shlib) { // Forward select _Shdr
         // Keep _Shdr for rtld data (below xct_off).
         // Discard _Shdr for compressed regions, except ".text" for gdb.
PATCH
cmake -S "$source_dir" -B "$source_dir/build" \
  -DCMAKE_BUILD_TYPE=Release -DUPX_CONFIG_DISABLE_WERROR=ON
cmake --build "$source_dir/build" --parallel 4
mkdir -p "$source_dir/bin"
install -m 755 "$source_dir/build/upx" "$source_dir/bin/upx"
if [ -n "${GITHUB_PATH:-}" ]; then
  printf '%s\n' "$source_dir/bin" >> "$GITHUB_PATH"
fi
"$source_dir/bin/upx" --version
