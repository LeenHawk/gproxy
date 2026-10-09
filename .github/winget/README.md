# WinGet packages

The checked-in 4.0.0 manifests record the original portable ZIP submissions.
New CI submissions use trusted, signed MSIX installers exclusively.
A manifest pins a specific released version, URL and SHA-256; older manifests
must not be rewritten to point at a newer binary.

| Edition | Package identifier | Command |
| --- | --- | --- |
| Desktop | `LeenHawk.GPROXY.Desktop` | `gproxy-desktop` |
| CLI (with Web Console) | `LeenHawk.GPROXY.CLI` | `gproxy` |
| Headless (without Web Console) | `LeenHawk.GPROXY.Headless` | `gproxy-headless` |

Headless has the independent MSIX identity `LeenHawk.GPROXYHeadless` and the
`gproxy-headless.exe` execution alias, so it does not take over CLI's `gproxy.exe` alias. Its ZIP continues to contain `gproxy.exe`. Headless does
not require a Microsoft Store listing.

Desktop declares `Microsoft.EdgeWebView2Runtime`. Each edition supports x64 and
ARM64 with separate hashes, plus English, Simplified Chinese and Traditional
Chinese package metadata.

Submit each new package version in its own pull request to
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs). Validate the
manifests with the current WinGet schema and `winget validate`, and install-test
with `winget install --manifest <directory>` on Windows. Verify the downloaded
ZIP hashes and nested executable names before submission.

After Microsoft merges and publishes a version, install using an exact match:

```powershell
winget install --id LeenHawk.GPROXY.Desktop --exact --source winget
winget install --id LeenHawk.GPROXY.CLI --exact --source winget
winget install --id LeenHawk.GPROXY.Headless --exact --source winget
```

The Microsoft Store source is separate. Desktop and CLI are published there with
product IDs `9P2FJRB9RS4Z` (Desktop) and `9NBMH3S5K0L9` (CLI):

```powershell
winget install --id 9P2FJRB9RS4Z --source msstore
winget install --id 9NBMH3S5K0L9 --source msstore
```

Stable releases automatically call `winget-publish.yml` after release assets are
published. It verifies MSIX checksums, Authenticode trust and timestamps, identity and manifest schemas,
then submits one PR per edition using the `WINGET_TOKEN` repository secret.
Existing MSIX versions/open PRs are skipped. Unsigned MSIX packages are skipped;
release signing uses SignPath with unsigned fallback when unavailable. Manual
dispatch defaults to validation only (`dry_run`); see [configuration and maintenance](../../dev_docs/winget.md).

Headless uses version-independent metadata templates in `templates/Headless/`;
no historical Headless release is implied by these templates. The generator
requires x64 and ARM64 `gproxy-headless-windows-*.msix` assets and verifies their
identity, executable, distinct command alias and trusted signature before producing
manifests. Unsigned fallback MSIX files remain release artifacts but are skipped
by WinGet submission. Local preparation does not register or publish a WinGet package.
