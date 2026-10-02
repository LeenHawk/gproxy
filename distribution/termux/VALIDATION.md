# Termux validation — 2026-10-02

## Inputs

- GPROXY source: v4.0.3, archive SHA256
  `650989657602f478cc3367d0bcecc08266c6a653f23c11d1da917816e837b15d`.
- Termux packaging repository: `415ba6df2df27ef08d6c473655b872551fe826d0`.
- Official builder image: the immutable digest in `toolchain.json`.
- Rust 1.98.1, Node 24.18.0, pnpm 9.15.9, NDK r30, Android API 24.
- Upstream release compression: UPX 5.2.1, `--best --lzma`, followed by `--test`.

## Results

| Check | Result |
| --- | --- |
| Official recipe linter | Passed |
| Source archive checksum and patch application | Passed |
| x86_64 source build, DEB creation, ELF symbol checks | Passed |
| aarch64 source build, DEB creation, ELF symbol checks | Passed |
| Fat LTO release profile | Preserved |
| UPX compression/integrity, both architectures | Passed |
| Android 15 x86_64, fresh data directory | Passed |
| Replace v4.0.2 `gproxy-cli` with v4.0.3 `gproxy` using APT | Passed |
| Embedded Console, health endpoint and authenticated admin API | Passed |
| Existing admin API key works after restart/upgrade without re-bootstrap | Passed |
| CLI update/check and Console check/apply/rollback refuse package replacement | Passed |
| Installed executable unchanged by refused updates | Passed |
| GitHub workflow actionlint; GitLab YAML; generated CNB Android matrix | Passed locally |

The ordinary source-recipe DEBs were 14,316,532 bytes (x86_64) and 13,927,348
bytes (aarch64), both below 100 MiB. Runtime tests used the UPX-compressed
upstream package generated from this recipe. `readelf` confirmed the required
Termux OpenSSL and libc++ shared libraries and the Termux library RUNPATH.

Both fresh and upgrade runs ended with exit code 0 and:

```text
PASS: startup, Console, admin API, persisted key, package-managed updates
```

## Runtime environment and limits

Tests used Termux v0.118.3's GitHub debug x86_64 APK on an Android 15 emulator.
KVM was unavailable, so the emulator used software execution with an extended
watchdog timeout. Its original APK bootstrap was extracted using the official
installer's permission/symlink rules and its second-stage installer completed
successfully. Commands ran through Termux's own RunCommandService as application
UID 10209, SELinux context `untrusted_app_27`; SELinux remained enforcing.

`adb run-as` is not interchangeable with that context: its `runas_app` context
caused termux-exec to force Android-linker execution, which rejected the existing
UPX-packed v4.0.2 executable. The same executable succeeded in the normal Termux
application context. The passing runtime results above use that normal context.

The emulator could not resolve the external APT mirror during setup; package
installation used local DEBs and the APK's installed runtime dependencies
(libc++ 27c, OpenSSL 3.4.1, ca-certificates 2025.02.25). This proves local
installation and API operation, not external DNS/TLS or provider forwarding.
Local build networking used a Go module mirror and cached official tool archives;
source/tool checksums remained enabled.

ARM64 device execution, physical devices, Termux:Boot/reboot behavior and paid
upstream forwarding were not tested. Updated GitHub/GitLab/CNB release jobs have
not run remotely. No Termux repository submission or publication was performed.

Local logs, original recipe DEBs, compressed release packages and the emulator
are under `target/termux-validation/`; that directory is not a release attachment.
