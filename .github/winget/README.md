# WinGet packages

The checked-in 4.0.0 manifests record the original portable ZIP submissions.
New CI submissions use trusted, signed MSIX installers exclusively.
A manifest pins a specific released version, URL and SHA-256; older manifests
must not be rewritten to point at a newer binary.

| Edition | Package identifier | Command |
| --- | --- | --- |
| Desktop | `LeenHawk.GPROXY.Desktop` | `gproxy-desktop` |
| CLI | `LeenHawk.GPROXY.CLI` | `gproxy` |

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
```

The Microsoft Store source is separate. Both products are published there with
product IDs `9P2FJRB9RS4Z` (Desktop) and `9NBMH3S5K0L9` (CLI):

```powershell
winget install --id 9P2FJRB9RS4Z --source msstore
winget install --id 9NBMH3S5K0L9 --source msstore
```

Stable releases automatically call `winget-publish.yml` after release assets are
published. It verifies MSIX checksums, Authenticode trust and timestamps, identity and manifest schemas,
then submits one PR per edition using the `WINGET_TOKEN` repository secret.
Existing MSIX versions/open PRs are skipped. Unsigned releases are skipped. SignPath signs release MSIX packages before publication. Manual dispatch defaults to validation
only (`dry_run`); see [configuration and maintenance](../../dev_docs/winget.md).
