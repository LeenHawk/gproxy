# Reproducible builds

Follow the [F-Droid reproducible-build guide](https://f-droid.org/docs/Reproducible_Builds/).
The acceptance criterion is identical unsigned binaries and packages from two
independent clean builds of the same source with the same platform toolchain.
Publisher signatures, notarization and store re-signing are verified separately.

Build paths are inputs to the build environment. In particular, vendored OpenSSL
embeds its installation path and compiler options. Arbitrary checkout paths are
not an acceptance requirement: use the same absolute paths, as the guide recommends.
Do not remove differing bytes from the comparison to obtain a passing result.

`scripts/reproduce-build.py` runs both builds sequentially at these fixed paths:

| Input | Unix | Windows |
| --- | --- | --- |
| Source | `/tmp/gproxy-reproduce/source` | `C:/gproxy-reproduce/source` |
| Cargo home | `/tmp/gproxy-reproduce/cargo` | `C:/gproxy-reproduce/cargo` |
| Cargo target directory | `/tmp/gproxy-reproduce/source/target` | `C:/gproxy-reproduce/source/target` |

The root is created with private permissions and must not already exist. The
source and compiled output are removed between attempts. Only downloaded Cargo
dependencies are retained. No Cargo build cache is restored. `SOURCE_DATE_EPOCH`
comes from the source commit. Android SDK/NDK locations are fixed within each
toolchain environment; container builds use one unchanged image for both attempts.

Run a native check using the production build/package scripts:

```sh
python3 scripts/reproduce-build.py --result dist/reproducible -- \
  bash scripts/reproduce-dispatch.sh x86_64-unknown-linux-gnu headless
```

The `Reproducible builds` workflow derives its native target/product matrix from
`scripts/release-targets.json`, adds the Android store variants, Worker and
Serverless packages, and saves both outputs with a JSON comparison report.
It uses public update/Store identities, creates unsigned packages, and does not
publish a release or access production signing keys. A missing output, failed
build or byte difference is a failure. Passing an individual job is not proof
that all platforms are reproducible. The workflow also compares OCI images built from the independently built Linux
executables. The unreleased experimental iOS branch is outside this verification scope.

For F-Droid, build the F-Droid flavor rather than comparing it to the direct
distribution flavor: they have different updater settings. After byte-level
differences are resolved, use APK signature copying and signature verification
against an upstream APK signed with `apksigner`. Only then add `Binaries` (or
`Builds.binary`) and `AllowedAPKSigningKeys` to fdroiddata. A green build pipeline
alone is not evidence that signature copying passes.

Use `diffoscope` on failed pairs to locate differences. Apply the guide's
workarounds only when the actual difference requires them; do not indiscriminately
disable baseline profiles, shrinking, stripping or VCS metadata.

Android compilation uses build-tools 36.1.0 and NDK 30.0.15729638. APK signing
uses `apksigner` from build-tools 34.0.0, following the guide's warning about
signature-copy incompatibilities in newer signers. The F-Droid build mirrors
upstream's source/Cargo/NDK paths and explicitly selects Debian OpenJDK 21.

After obtaining the signed reference APK and an independent F-Droid rebuild:

```sh
python3 -m pip install apksigcopier==1.1.1
python3 scripts/mobile/verify-reproducible-apk.py reference.apk rebuilt.apk \
  --build-tools "$ANDROID_HOME/build-tools/34.0.0" \
  --expected-cert-sha256 "$EXPECTED_CERT_SHA256" \
  --report dist/reproducible-apk.json
```

The verifier requires the expected certificate, copies the reference signature,
and checks the rebuilt APK with `apksigner`. It does not ignore ZIP differences.
A successful check using a temporary test certificate does not establish that
an upstream-signed release has been published or accepted by F-Droid.
